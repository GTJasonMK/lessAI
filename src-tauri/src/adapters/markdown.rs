use super::TextRegion;

/// Markdown 适配器：识别“结构性高风险片段”，并将其标记为 `skip_rewrite`。
///
/// 设计目标：
/// - 让分块器只处理“可改写的自然语言正文”
/// - 代码块/表格/front matter 等内容原样保留，避免模型改坏格式或语义
/// - 输出必须严格保真：拼回去后与原文完全一致
pub struct MarkdownAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FenceMarker {
    ch: char,
    len: usize,
}

#[derive(Debug, Clone, Copy)]
struct LineSlice<'a> {
    line: &'a str,
    full: &'a str,
}

impl MarkdownAdapter {
    pub fn split_regions(text: &str, rewrite_headings: bool) -> Vec<TextRegion> {
        let lines = split_lines_with_endings(text);
        let front_matter = find_yaml_front_matter_range(&lines);
        let mut regions: Vec<TextRegion> = Vec::new();

        let mut buffer = String::new();
        let mut in_fence: Option<FenceMarker> = None;

        let mut flush = |regions: &mut Vec<TextRegion>, buffer: &mut String, skip: bool| {
            if buffer.is_empty() {
                return;
            }
            regions.push(TextRegion {
                body: std::mem::take(buffer),
                skip_rewrite: skip,
            });
        };

        let mut index = 0usize;
        while index < lines.len() {
            if in_fence.is_none() {
                if let Some((start, end)) = front_matter {
                    if index == start {
                        flush(&mut regions, &mut buffer, false);
                        let mut fm = String::new();
                        for slice in &lines[start..=end] {
                            fm.push_str(slice.full);
                        }
                        regions.push(TextRegion {
                            body: fm,
                            skip_rewrite: true,
                        });
                        index = end + 1;
                        continue;
                    }
                }

                if index + 1 < lines.len()
                    && !lines[index].line.trim().is_empty()
                    && lines[index].line.contains('|')
                    && is_markdown_table_delimiter(lines[index + 1].line)
                {
                    flush(&mut regions, &mut buffer, false);
                    let mut table = String::new();
                    table.push_str(lines[index].full);
                    table.push_str(lines[index + 1].full);
                    index += 2;

                    while index < lines.len() {
                        let line = lines[index].line;
                        if line.trim().is_empty() {
                            break;
                        }
                        if detect_fence_marker(line).is_some() {
                            break;
                        }
                        if !line.contains('|') {
                            break;
                        }
                        table.push_str(lines[index].full);
                        index += 1;
                    }

                    regions.push(TextRegion {
                        body: table,
                        skip_rewrite: true,
                    });
                    continue;
                }
            }

            let line = lines[index].line;
            let full = lines[index].full;

            if let Some(marker) = in_fence {
                buffer.push_str(full);
                if is_fence_close(line, marker) {
                    flush(&mut regions, &mut buffer, true);
                    in_fence = None;
                }
                index += 1;
                continue;
            }

            if let Some(marker) = detect_fence_marker(line) {
                flush(&mut regions, &mut buffer, false);
                buffer.push_str(full);
                in_fence = Some(marker);
                index += 1;
                continue;
            }

            buffer.push_str(full);
            index += 1;
        }

        if !buffer.is_empty() {
            flush(&mut regions, &mut buffer, in_fence.is_some());
        }

        // 二次处理：在“可改写区域”内再保护高风险行/内联结构（链接/内联代码等）。
        // 注意：必须严格保真（regions 拼回去 == 原文）。
        let mut out: Vec<TextRegion> = Vec::new();
        for region in regions.into_iter() {
            if region.body.is_empty() {
                continue;
            }

            if region.skip_rewrite {
                push_text_region(&mut out, region);
                continue;
            }

            let pieces = split_inline_protected_regions(&region.body, rewrite_headings);
            for piece in pieces.into_iter() {
                push_text_region(&mut out, piece);
            }
        }

        out
    }
}

fn find_unescaped_emphasis_marker(
    text: &str,
    marker: u8,
    marker_len: usize,
    mut from: usize,
) -> Option<usize> {
    let bytes = text.as_bytes();
    if marker_len == 1 {
        while from < bytes.len() {
            if bytes[from] != marker {
                from += 1;
                continue;
            }
            if from > 0 && bytes[from - 1] == b'\\' {
                from += 1;
                continue;
            }
            if (from > 0 && bytes[from - 1] == marker)
                || (from + 1 < bytes.len() && bytes[from + 1] == marker)
            {
                from += 1;
                continue;
            }
            return Some(from);
        }
        None
    } else {
        while from + 1 < bytes.len() {
            if bytes[from] != marker || bytes[from + 1] != marker {
                from += 1;
                continue;
            }
            if from > 0 && bytes[from - 1] == b'\\' {
                from += 2;
                continue;
            }
            return Some(from);
        }
        None
    }
}

fn push_text_region(regions: &mut Vec<TextRegion>, region: TextRegion) {
    if region.body.is_empty() {
        return;
    }

    if let Some(last) = regions.last_mut() {
        if last.skip_rewrite == region.skip_rewrite {
            last.body.push_str(&region.body);
            return;
        }
    }

    regions.push(region);
}

fn split_lines_with_endings(text: &str) -> Vec<LineSlice<'_>> {
    let bytes = text.as_bytes();
    let mut lines: Vec<LineSlice<'_>> = Vec::new();

    let mut start = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'\n' => {
                lines.push(LineSlice {
                    line: &text[start..index],
                    full: &text[start..index + 1],
                });
                index += 1;
                start = index;
            }
            b'\r' => {
                if index + 1 < bytes.len() && bytes[index + 1] == b'\n' {
                    lines.push(LineSlice {
                        line: &text[start..index],
                        full: &text[start..index + 2],
                    });
                    index += 2;
                    start = index;
                } else {
                    lines.push(LineSlice {
                        line: &text[start..index],
                        full: &text[start..index + 1],
                    });
                    index += 1;
                    start = index;
                }
            }
            _ => index += 1,
        }
    }

    if start < bytes.len() {
        lines.push(LineSlice {
            line: &text[start..bytes.len()],
            full: &text[start..bytes.len()],
        });
    } else if text.is_empty() {
        lines.push(LineSlice { line: "", full: "" });
    }

    lines
}

fn detect_fence_marker(line: &str) -> Option<FenceMarker> {
    let trimmed = line.trim_start();
    let Some(first) = trimmed.chars().next() else {
        return None;
    };
    if first != '`' && first != '~' {
        return None;
    }

    let mut len = 0usize;
    for ch in trimmed.chars() {
        if ch == first {
            len = len.saturating_add(1);
        } else {
            break;
        }
    }

    if len >= 3 {
        Some(FenceMarker { ch: first, len })
    } else {
        None
    }
}

fn is_fence_close(line: &str, marker: FenceMarker) -> bool {
    let trimmed = line.trim_start();
    let mut count = 0usize;
    let mut end = 0usize;

    for (offset, ch) in trimmed.char_indices() {
        if ch == marker.ch {
            count = count.saturating_add(1);
            end = offset + ch.len_utf8();
        } else {
            break;
        }
    }

    if count < marker.len {
        return false;
    }

    trimmed[end..].trim().is_empty()
}

fn is_yaml_front_matter_open(line: &str) -> bool {
    let trimmed = line.trim_start_matches('\u{feff}').trim();
    trimmed == "---"
}

fn is_yaml_front_matter_close(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed == "---" || trimmed == "..."
}

fn find_yaml_front_matter_range(lines: &[LineSlice<'_>]) -> Option<(usize, usize)> {
    const MAX_FRONT_MATTER_LINES: usize = 200;

    let mut index = 0usize;
    while index < lines.len() && lines[index].line.trim().is_empty() {
        index += 1;
    }
    if index >= lines.len() {
        return None;
    }
    if !is_yaml_front_matter_open(lines[index].line) {
        return None;
    }

    let start = index;
    let end_limit = (start + MAX_FRONT_MATTER_LINES).min(lines.len().saturating_sub(1));
    index += 1;
    while index <= end_limit {
        if is_yaml_front_matter_close(lines[index].line) {
            return Some((start, index));
        }
        index += 1;
    }

    // 没有闭合标记：更像是 Markdown 水平线，不当作 front matter。
    None
}

fn is_markdown_table_delimiter(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    if !trimmed.contains('|') {
        return false;
    }
    let dash_count = trimmed.chars().filter(|ch| *ch == '-').count();
    if dash_count < 3 {
        return false;
    }

    trimmed.chars().all(|ch| match ch {
        '|' | '-' | ':' => true,
        _ => ch.is_whitespace(),
    })
}

fn split_inline_protected_regions(text: &str, rewrite_headings: bool) -> Vec<TextRegion> {
    let lines = split_lines_with_endings(text);
    let mut out: Vec<TextRegion> = Vec::new();
    let mut in_indented_code_block = false;
    let mut in_html_comment = false;

    let mut index = 0usize;
    while index < lines.len() {
        let slice = &lines[index];
        let mut line = slice.line;
        let full = slice.full;
        let ending = &full[slice.line.len()..];
        let mut emitted_prefix = false;

        if in_html_comment {
            if let Some(close_at) = line.find("-->") {
                let close_end = close_at + "-->".len();
                if close_end > 0 {
                    push_text_region(
                        &mut out,
                        TextRegion {
                            body: line[..close_end].to_string(),
                            skip_rewrite: true,
                        },
                    );
                    emitted_prefix = true;
                }
                line = &line[close_end..];
                in_html_comment = false;
            } else {
                push_text_region(
                    &mut out,
                    TextRegion {
                        body: full.to_string(),
                        skip_rewrite: true,
                    },
                );
                in_indented_code_block = false;
                index += 1;
                continue;
            }
        }

        // 如果本行出现未闭合的 HTML 注释，后半截整行都应跳过（并进入跨行状态）。
        if let Some(open_at) = line.find("<!--") {
            let after_open = open_at + "<!--".len();
            if line[after_open..].find("-->").is_none() {
                let (before, comment) = line.split_at(open_at);
                if !before.is_empty() {
                    process_markdown_line(&mut out, before, "");
                }
                push_text_region(
                    &mut out,
                    TextRegion {
                        body: format!("{comment}{ending}"),
                        skip_rewrite: true,
                    },
                );
                in_html_comment = true;
                in_indented_code_block = false;
                index += 1;
                continue;
            }
        }

        // 参考式链接定义：`[id]: https://...`
        if is_reference_definition_line(line) {
            push_text_region(
                &mut out,
                TextRegion {
                    body: if emitted_prefix {
                        format!("{line}{ending}")
                    } else {
                        full.to_string()
                    },
                    skip_rewrite: true,
                },
            );
            in_indented_code_block = false;
            index += 1;
            continue;
        }

        // HTML 块/行内 HTML：保守跳过整行，避免属性/标签被模型破坏。
        if is_html_like_line(line) {
            push_text_region(
                &mut out,
                TextRegion {
                    body: if emitted_prefix {
                        format!("{line}{ending}")
                    } else {
                        full.to_string()
                    },
                    skip_rewrite: true,
                },
            );
            in_indented_code_block = false;
            index += 1;
            continue;
        }

        // 水平线：属于纯格式，跳过。
        if is_horizontal_rule_line(line) {
            push_text_region(
                &mut out,
                TextRegion {
                    body: if emitted_prefix {
                        format!("{line}{ending}")
                    } else {
                        full.to_string()
                    },
                    skip_rewrite: true,
                },
            );
            in_indented_code_block = false;
            index += 1;
            continue;
        }

        // 缩进代码块（4 空格 / tab）：
        // - 这是 Markdown 常见的代码块写法（不一定有 ```）
        // - 一旦进入代码块，直到遇到“非缩进且非空行”才退出
        if in_indented_code_block {
            if line.trim().is_empty() {
                // 空行：保守起见算作代码块结束（避免后续长段被误跳过）
                in_indented_code_block = false;
            } else if is_indented_code_line(line) {
                push_text_region(
                    &mut out,
                    TextRegion {
                        body: if emitted_prefix {
                            format!("{line}{ending}")
                        } else {
                            full.to_string()
                        },
                        skip_rewrite: true,
                    },
                );
                index += 1;
                continue;
            } else {
                in_indented_code_block = false;
            }
        }

        if is_indented_code_line(line) {
            in_indented_code_block = true;
            push_text_region(
                &mut out,
                TextRegion {
                    body: if emitted_prefix {
                        format!("{line}{ending}")
                    } else {
                        full.to_string()
                    },
                    skip_rewrite: true,
                },
            );
            index += 1;
            continue;
        }

        if !rewrite_headings {
            let next_line = lines.get(index + 1).map(|slice| slice.line);
            let is_setext_heading = !line.trim().is_empty()
                && next_line.is_some_and(|next| is_setext_underline_line(next));
            let is_atx_heading = is_atx_heading_line(line);
            if is_atx_heading || is_setext_heading {
                push_text_region(
                    &mut out,
                    TextRegion {
                        body: if emitted_prefix {
                            format!("{line}{ending}")
                        } else {
                            full.to_string()
                        },
                        skip_rewrite: true,
                    },
                );
                in_indented_code_block = false;
                index += 1;
                continue;
            }
        }

        process_markdown_line(&mut out, line, ending);
        index += 1;
    }

    out
}

fn process_markdown_line(out: &mut Vec<TextRegion>, line: &str, ending: &str) {
    let prefix_len = markdown_line_prefix_len(line);
    let (prefix, core) = if prefix_len > 0 && prefix_len <= line.len() {
        (&line[..prefix_len], &line[prefix_len..])
    } else {
        ("", line)
    };

    if !prefix.is_empty() {
        push_text_region(
            out,
            TextRegion {
                body: prefix.to_string(),
                skip_rewrite: true,
            },
        );
    }

    let spans = find_markdown_protected_spans(core);
    if spans.is_empty() {
        push_rewriteable_markdown_text(out, core);
        append_line_ending(out, ending);
        return;
    }

    let mut cursor = 0usize;
    for (start, end) in spans.into_iter() {
        if start > cursor {
            push_rewriteable_markdown_text(out, &core[cursor..start]);
        }
        push_text_region(
            out,
            TextRegion {
                body: core[start..end].to_string(),
                skip_rewrite: true,
            },
        );
        cursor = end;
    }
    if cursor < core.len() {
        push_rewriteable_markdown_text(out, &core[cursor..]);
    }

    append_line_ending(out, ending);
}

fn append_line_ending(out: &mut Vec<TextRegion>, ending: &str) {
    if ending.is_empty() {
        return;
    }
    if let Some(last) = out.last_mut() {
        last.body.push_str(ending);
    } else {
        out.push(TextRegion {
            body: ending.to_string(),
            skip_rewrite: true,
        });
    }
}

fn push_rewriteable_markdown_text(out: &mut Vec<TextRegion>, text: &str) {
    if text.is_empty() {
        return;
    }

    // 保护强调语法本身（** __ ~~ * _），但允许内部正文改写。
    //
    // 注意：这里必须在“字节索引”层面扫描，且只在命中 ASCII 标记时才切片，
    // 否则遇到中文等多字节字符会因为 UTF-8 边界不合法而 panic。
    let bytes = text.as_bytes();

    let mut cursor = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        let (marker, len) = match bytes[index] {
            b'*' => {
                if index + 1 < bytes.len() && bytes[index + 1] == b'*' {
                    ("**", 2usize)
                } else {
                    ("*", 1usize)
                }
            }
            b'_' => {
                if index + 1 < bytes.len() && bytes[index + 1] == b'_' {
                    ("__", 2usize)
                } else {
                    ("_", 1usize)
                }
            }
            b'~' => {
                if index + 1 < bytes.len() && bytes[index + 1] == b'~' {
                    ("~~", 2usize)
                } else {
                    index += 1;
                    continue;
                }
            }
            _ => {
                index += 1;
                continue;
            }
        };

        // 反斜杠转义：\* \_ \~~ 等不视为强调标记
        if index > 0 && bytes[index - 1] == b'\\' {
            index += len;
            continue;
        }

        // 单字符强调：做一些启发式，减少误判（但仍以“宁可多保护”为主）。
        if len == 1 {
            if index + 1 >= bytes.len() || bytes[index + 1].is_ascii_whitespace() {
                index += 1;
                continue;
            }
            // 避免把 `**` / `__` 当成两个单字符
            if index + 1 < bytes.len() && bytes[index + 1] == bytes[index] {
                index += 1;
                continue;
            }
            // `_` 在词内（foo_bar）通常不是强调，避免误判
            if bytes[index] == b'_' {
                let prev = if index > 0 { bytes[index - 1] } else { b' ' };
                let next = bytes[index + 1];
                if prev.is_ascii_alphanumeric() && next.is_ascii_alphanumeric() {
                    index += 1;
                    continue;
                }
            }
        }

        let open = index;
        let search_from = open + len;
        let Some(close) = find_unescaped_emphasis_marker(text, bytes[open], len, search_from)
        else {
            index += len;
            continue;
        };

        // 关闭标记也做启发式校验：避免 `* foo *` 之类误判。
        if len == 1 {
            if close == 0 || bytes[close - 1].is_ascii_whitespace() {
                index += 1;
                continue;
            }
            if close + 1 < bytes.len() && bytes[close + 1] == bytes[open] {
                index += 1;
                continue;
            }
            if bytes[open] == b'_' {
                let prev = bytes[close - 1];
                let next = if close + 1 < bytes.len() {
                    bytes[close + 1]
                } else {
                    b' '
                };
                if prev.is_ascii_alphanumeric() && next.is_ascii_alphanumeric() {
                    index += 1;
                    continue;
                }
            }

            let inner_start = open + 1;
            let inner_end = close;
            if inner_end <= inner_start {
                index += 1;
                continue;
            }
            if bytes[inner_start].is_ascii_whitespace()
                || bytes[inner_end.saturating_sub(1)].is_ascii_whitespace()
            {
                index += 1;
                continue;
            }
        } else if close <= open + len {
            index += len;
            continue;
        }

        if open > cursor {
            push_text_region(
                out,
                TextRegion {
                    body: text[cursor..open].to_string(),
                    skip_rewrite: false,
                },
            );
        }

        push_text_region(
            out,
            TextRegion {
                body: marker.to_string(),
                skip_rewrite: true,
            },
        );

        let inner_start = open + len;
        let inner_end = close;
        if inner_end > inner_start {
            // 允许内部再次递归保护（例如 **bold _italic_**）。
            push_rewriteable_markdown_text(out, &text[inner_start..inner_end]);
        }

        push_text_region(
            out,
            TextRegion {
                body: marker.to_string(),
                skip_rewrite: true,
            },
        );

        cursor = close + len;
        index = cursor;
    }

    if cursor < text.len() {
        push_text_region(
            out,
            TextRegion {
                body: text[cursor..].to_string(),
                skip_rewrite: false,
            },
        );
    }
}

fn is_reference_definition_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('[') {
        return false;
    }
    let bytes = trimmed.as_bytes();
    let mut index = 1usize;
    while index < bytes.len() {
        if bytes[index] == b']' {
            break;
        }
        index += 1;
    }
    if index >= bytes.len() {
        return false;
    }
    if index + 1 >= bytes.len() || bytes[index + 1] != b':' {
        return false;
    }

    let rest = trimmed[index + 2..].trim_start();
    !rest.is_empty()
}

fn is_html_like_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('<') {
        return false;
    }
    let bytes = trimmed.as_bytes();
    if bytes.len() < 2 {
        return false;
    }
    matches!(bytes[1], b'/' | b'!' | b'?' | b'a'..=b'z' | b'A'..=b'Z')
}

fn is_horizontal_rule_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    let mut ch_iter = trimmed.chars();
    let Some(first) = ch_iter.next() else {
        return false;
    };
    if !matches!(first, '-' | '*' | '_') {
        return false;
    }

    let count = trimmed.chars().filter(|ch| *ch == first).count();
    if count < 3 {
        return false;
    }

    trimmed.chars().all(|ch| ch == first || ch.is_whitespace())
}

fn is_atx_heading_line(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut pos = 0usize;
    while pos < bytes.len() && matches!(bytes[pos], b' ' | b'\t') {
        pos += 1;
    }
    if pos >= bytes.len() || bytes[pos] != b'#' {
        return false;
    }

    let mut p = pos;
    while p < bytes.len() && bytes[p] == b'#' {
        p += 1;
    }
    let count = p - pos;
    if !(1..=6).contains(&count) {
        return false;
    }
    if p >= bytes.len() {
        return true;
    }
    bytes[p].is_ascii_whitespace()
}

fn is_setext_underline_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    let mut chars = trimmed.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !matches!(first, '-' | '=') {
        return false;
    }
    chars.all(|ch| ch == first)
}

fn strip_one_indent(line: &str) -> Option<&str> {
    if line.starts_with('\t') {
        return Some(&line[1..]);
    }
    let bytes = line.as_bytes();
    if bytes.len() >= 4 && bytes[0..4] == [b' ', b' ', b' ', b' '] {
        return Some(&line[4..]);
    }
    None
}

fn remainder_starts_with_list_or_quote(rem: &str) -> bool {
    let trimmed = rem.trim_start();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.starts_with('>') || trimmed.starts_with('#') {
        return true;
    }
    if trimmed.starts_with("- ")
        || trimmed.starts_with("* ")
        || trimmed.starts_with("+ ")
        || trimmed.starts_with("---")
        || trimmed.starts_with("***")
        || trimmed.starts_with("___")
    {
        return true;
    }

    let bytes = trimmed.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }
    index > 0
        && index + 1 < bytes.len()
        && (bytes[index] == b'.' || bytes[index] == b')')
        && bytes[index + 1].is_ascii_whitespace()
}

fn is_indented_code_line(line: &str) -> bool {
    if line.trim().is_empty() {
        return false;
    }
    let Some(rem) = strip_one_indent(line) else {
        return false;
    };

    // 如果缩进后看起来仍是列表/引用/标题等结构，则更像嵌套 Markdown，而不是代码块。
    if remainder_starts_with_list_or_quote(rem) {
        return false;
    }

    true
}

fn markdown_line_prefix_len(line: &str) -> usize {
    let bytes = line.as_bytes();
    let mut pos = 0usize;
    while pos < bytes.len() && matches!(bytes[pos], b' ' | b'\t') {
        pos += 1;
    }

    let indent_end = pos;
    if indent_end >= bytes.len() {
        return 0;
    }

    // 脚注定义：[^id]: ...
    if bytes[indent_end] == b'[' && indent_end + 2 < bytes.len() && bytes[indent_end + 1] == b'^' {
        let mut p = indent_end + 2;
        while p < bytes.len() {
            if bytes[p] == b']' {
                break;
            }
            p += 1;
        }
        if p + 1 < bytes.len() && bytes[p] == b']' && bytes[p + 1] == b':' {
            p += 2;
            while p < bytes.len() && bytes[p].is_ascii_whitespace() {
                p += 1;
            }
            return p;
        }
    }

    // ATX 标题：### 标题
    if bytes[indent_end] == b'#' {
        let mut p = indent_end;
        while p < bytes.len() && bytes[p] == b'#' {
            p += 1;
        }
        let count = p - indent_end;
        if (1..=6).contains(&count) && p < bytes.len() && bytes[p].is_ascii_whitespace() {
            while p < bytes.len() && bytes[p].is_ascii_whitespace() {
                p += 1;
            }
            return p;
        }
    }

    // 引用：>>> ...
    if bytes[indent_end] == b'>' {
        let mut p = indent_end;
        while p < bytes.len() && bytes[p] == b'>' {
            p += 1;
        }
        while p < bytes.len() && bytes[p].is_ascii_whitespace() {
            p += 1;
        }
        return p;
    }

    // 无序列表：-/*/+ + 空白
    if matches!(bytes[indent_end], b'-' | b'*' | b'+') {
        let mut p = indent_end + 1;
        if p < bytes.len() && bytes[p].is_ascii_whitespace() {
            while p < bytes.len() && bytes[p].is_ascii_whitespace() {
                p += 1;
            }

            // 任务列表：- [ ] / - [x]
            if p + 2 < bytes.len() && bytes[p] == b'[' && bytes[p + 2] == b']' {
                let mid = bytes[p + 1];
                if matches!(mid, b' ' | b'x' | b'X') {
                    p += 3;
                    while p < bytes.len() && bytes[p].is_ascii_whitespace() {
                        p += 1;
                    }
                }
            }

            return p;
        }
    }

    // 有序列表：1. / 1)
    let mut p = indent_end;
    while p < bytes.len() && bytes[p].is_ascii_digit() {
        p += 1;
    }
    if p > indent_end
        && p + 1 < bytes.len()
        && matches!(bytes[p], b'.' | b')')
        && bytes[p + 1].is_ascii_whitespace()
    {
        p += 1;
        while p < bytes.len() && bytes[p].is_ascii_whitespace() {
            p += 1;
        }
        return p;
    }

    0
}

fn count_run(bytes: &[u8], start: usize, target: u8) -> usize {
    let mut len = 0usize;
    let mut index = start;
    while index < bytes.len() && bytes[index] == target {
        len = len.saturating_add(1);
        index += 1;
    }
    len
}

fn find_backtick_closing(bytes: &[u8], from: usize, run_len: usize) -> Option<usize> {
    if run_len == 0 {
        return None;
    }
    let mut index = from;
    while index + run_len <= bytes.len() {
        if bytes[index] == b'`' {
            let candidate = count_run(bytes, index, b'`');
            if candidate == run_len {
                return Some(index + run_len);
            }
            index += candidate.max(1);
            continue;
        }
        index += 1;
    }
    None
}

fn find_matching_paren(line: &str, start: usize) -> Option<usize> {
    let bytes = line.as_bytes();
    if start >= bytes.len() || bytes[start] != b'(' {
        return None;
    }
    let mut depth = 1usize;
    let mut index = start + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index = (index + 2).min(bytes.len()),
            b'(' => {
                depth = depth.saturating_add(1);
                index += 1;
            }
            b')' => {
                depth = depth.saturating_sub(1);
                index += 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => index += 1,
        }
    }
    None
}

fn find_matching_bracket(line: &str, start: usize) -> Option<usize> {
    let bytes = line.as_bytes();
    if start >= bytes.len() || bytes[start] != b'[' {
        return None;
    }
    let mut depth = 1usize;
    let mut index = start + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index = (index + 2).min(bytes.len()),
            b'[' => {
                depth = depth.saturating_add(1);
                index += 1;
            }
            b']' => {
                depth = depth.saturating_sub(1);
                index += 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => index += 1,
        }
    }
    None
}

fn find_markdown_link_end(line: &str, start: usize) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut index = start;
    if index >= bytes.len() {
        return None;
    }

    if bytes[index] == b'!' {
        if index + 1 >= bytes.len() || bytes[index + 1] != b'[' {
            return None;
        }
        index += 1;
    }

    if bytes[index] != b'[' {
        return None;
    }

    let close = find_matching_bracket(line, index)?;
    let mut pos = close;
    while pos < bytes.len() && matches!(bytes[pos], b' ' | b'\t') {
        pos += 1;
    }
    if pos >= bytes.len() {
        return None;
    }

    match bytes[pos] {
        b'(' => find_matching_paren(line, pos),
        b'[' => find_matching_bracket(line, pos),
        _ => None,
    }
}

fn find_autolink_end(line: &str, start: usize) -> Option<usize> {
    let bytes = line.as_bytes();
    if start >= bytes.len() || bytes[start] != b'<' {
        return None;
    }
    let mut index = start + 1;
    while index < bytes.len() {
        if bytes[index] == b'>' {
            let inner = &line[start + 1..index];
            let lower = inner.to_ascii_lowercase();
            if lower.starts_with("http://")
                || lower.starts_with("https://")
                || lower.starts_with("mailto:")
            {
                return Some(index + 1);
            }
            return None;
        }
        index += 1;
    }
    None
}

fn find_html_comment_end(line: &str, start: usize) -> Option<usize> {
    let bytes = line.as_bytes();
    if start >= bytes.len() {
        return None;
    }
    if !line[start..].starts_with("<!--") {
        return None;
    }
    let from = start + "<!--".len();
    line[from..]
        .find("-->")
        .map(|offset| from + offset + "-->".len())
}

fn find_inline_html_tag_end(line: &str, start: usize) -> Option<usize> {
    let bytes = line.as_bytes();
    if start >= bytes.len() || bytes[start] != b'<' {
        return None;
    }

    // 这里仅保护“标签本体”，不试图解析完整 HTML。
    // 规则偏保守：`<` 后必须是 `</` 或字母开头的标签名，且在同一行出现 `>`。
    let mut pos = start + 1;
    if pos >= bytes.len() {
        return None;
    }
    if !(bytes[pos] == b'/' || bytes[pos].is_ascii_alphabetic()) {
        return None;
    }

    let mut in_single = false;
    let mut in_double = false;
    while pos < bytes.len() {
        match bytes[pos] {
            b'\'' if !in_double => in_single = !in_single,
            b'"' if !in_single => in_double = !in_double,
            b'>' if !in_single && !in_double => return Some(pos + 1),
            b'\n' | b'\r' => return None,
            _ => {}
        }
        pos += 1;
    }

    None
}

fn find_bare_url_end(line: &str, start: usize) -> usize {
    let bytes = line.as_bytes();
    let mut end = start;
    while end < bytes.len() && !bytes[end].is_ascii_whitespace() {
        end += 1;
    }

    // 去掉结尾常见标点（避免把 `)` `,` 等一起保护导致改写区断裂过大）。
    while end > start {
        match bytes[end - 1] {
            b'.' | b',' | b';' | b':' | b'!' | b'?' | b')' | b']' | b'}' | b'"' | b'\'' => {
                end -= 1;
            }
            _ => break,
        }
    }

    end.max(start)
}

fn find_markdown_protected_spans(line: &str) -> Vec<(usize, usize)> {
    let bytes = line.as_bytes();
    let mut spans: Vec<(usize, usize)> = Vec::new();

    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'`' => {
                let run_len = count_run(bytes, index, b'`');
                if let Some(end) = find_backtick_closing(bytes, index + run_len, run_len) {
                    spans.push((index, end));
                    index = end;
                    continue;
                }
                index += run_len.max(1);
            }
            b'!' => {
                if index + 1 < bytes.len() && bytes[index + 1] == b'[' {
                    if let Some(end) = find_markdown_link_end(line, index) {
                        spans.push((index, end));
                        index = end;
                        continue;
                    }
                }
                index += 1;
            }
            b'[' => {
                if let Some(end) = find_markdown_link_end(line, index) {
                    spans.push((index, end));
                    index = end;
                    continue;
                }
                // 脚注引用：[^id]
                if index + 1 < bytes.len() && bytes[index + 1] == b'^' {
                    if let Some(end) = find_matching_bracket(line, index) {
                        spans.push((index, end));
                        index = end;
                        continue;
                    }
                }
                // Pandoc 引用：[@doe2020] / [-@doe2020]
                if index + 1 < bytes.len()
                    && (bytes[index + 1] == b'@'
                        || (bytes[index + 1] == b'-'
                            && index + 2 < bytes.len()
                            && bytes[index + 2] == b'@'))
                {
                    if let Some(end) = find_matching_bracket(line, index) {
                        spans.push((index, end));
                        index = end;
                        continue;
                    }
                }
                index += 1;
            }
            b'<' => {
                if let Some(end) = find_html_comment_end(line, index) {
                    spans.push((index, end));
                    index = end;
                    continue;
                }
                if let Some(end) = find_autolink_end(line, index) {
                    spans.push((index, end));
                    index = end;
                    continue;
                }
                if let Some(end) = find_inline_html_tag_end(line, index) {
                    spans.push((index, end));
                    index = end;
                    continue;
                }
                index += 1;
            }
            b'h' => {
                if line[index..].starts_with("http://") || line[index..].starts_with("https://") {
                    let end = find_bare_url_end(line, index);
                    spans.push((index, end));
                    index = end;
                    continue;
                }
                index += 1;
            }
            b'w' => {
                if line[index..].starts_with("www.") {
                    let end = find_bare_url_end(line, index);
                    spans.push((index, end));
                    index = end;
                    continue;
                }
                index += 1;
            }
            _ => index += 1,
        }
    }

    spans
}

#[cfg(test)]
mod tests {
    use super::MarkdownAdapter;

    #[test]
    fn preserves_text_when_splitting_markdown_regions() {
        let text = "---\ntitle: 测试\n---\n\n# 标题\n\n这里是正文，包含 `inline code` 和 [链接](https://example.com)。\n\n```rust\nfn main() {}\n```\n\n|a|b|\n|---|---|\n|1|2|\n";
        let regions = MarkdownAdapter::split_regions(text, false);
        let rebuilt = regions
            .iter()
            .map(|region| region.body.as_str())
            .collect::<String>();
        assert_eq!(rebuilt, text);
        assert!(regions.iter().any(|r| r.skip_rewrite));
    }

    #[test]
    fn protects_inline_html_tags_and_single_emphasis_markers() {
        let text = "按 <kbd>Ctrl</kbd> + <kbd>S</kbd> 保存，这是 *重点* 和 _斜体_。\n下一行。";
        let regions = MarkdownAdapter::split_regions(text, false);
        let rebuilt = regions
            .iter()
            .map(|region| region.body.as_str())
            .collect::<String>();
        assert_eq!(rebuilt, text);

        assert!(regions
            .iter()
            .any(|r| r.skip_rewrite && r.body.contains("<kbd")));
        assert!(regions.iter().any(|r| r.skip_rewrite && r.body == "*"));
        assert!(regions
            .iter()
            .any(|r| !r.skip_rewrite && r.body.contains("重点")));
        assert!(regions.iter().any(|r| r.skip_rewrite && r.body == "_"));
        assert!(regions
            .iter()
            .any(|r| !r.skip_rewrite && r.body.contains("斜体")));
    }

    #[test]
    fn does_not_treat_intraword_underscore_as_emphasis() {
        let text = "foo_bar_baz";
        let regions = MarkdownAdapter::split_regions(text, false);
        let rebuilt = regions
            .iter()
            .map(|region| region.body.as_str())
            .collect::<String>();
        assert_eq!(rebuilt, text);
        assert!(!regions.iter().any(|r| r.skip_rewrite && r.body == "_"));
    }
}
