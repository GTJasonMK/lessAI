use crate::adapters::tex::TexAdapter;
use crate::models::ChunkPreset;

use super::boundary::{
    is_clause_boundary, is_closing_punctuation, is_sentence_boundary, BoundaryKind,
};
use super::{ParagraphBlock, SegmentedChunk};

fn is_tex_heading_command_span(body: &str) -> bool {
    let trimmed = body.trim_start();
    if !trimmed.starts_with('\\') {
        return false;
    }
    let lowered = trimmed.to_ascii_lowercase();
    const HEADINGS: &[&str] = &[
        "\\section",
        "\\subsection",
        "\\subsubsection",
        "\\paragraph",
        "\\subparagraph",
        "\\chapter",
        "\\part",
        "\\title",
        "\\subtitle",
        "\\caption",
    ];

    HEADINGS.iter().any(|prefix| {
        if !lowered.starts_with(prefix) {
            return false;
        }
        let rest = &lowered[prefix.len()..];
        rest.is_empty()
            || rest.starts_with('*')
            || rest
                .chars()
                .next()
                .is_some_and(|ch| ch.is_whitespace() || ch == '[' || ch == '{')
    })
}

fn is_tex_comment_span(body: &str) -> bool {
    body.trim_start().starts_with('%')
}

fn is_tex_math_environment_name(name: &str) -> bool {
    // 与 TexAdapter::math_env_names 保持一致（仅用于“是否把数学块并回相邻段落”的启发式）。
    matches!(
        name,
        "equation"
            | "equation*"
            | "align"
            | "align*"
            | "alignat"
            | "alignat*"
            | "flalign"
            | "flalign*"
            | "gather"
            | "gather*"
            | "multline"
            | "multline*"
            | "eqnarray"
            | "eqnarray*"
            | "math"
            | "displaymath"
            | "split"
            | "cases"
            | "matrix"
            | "pmatrix"
            | "bmatrix"
            | "vmatrix"
            | "Vmatrix"
    )
}

fn is_tex_math_skip_block_span(body: &str) -> bool {
    let trimmed = body.trim_start();

    if trimmed.starts_with("$$") {
        return true;
    }
    if trimmed.starts_with("\\[") {
        return true;
    }

    if !trimmed.starts_with("\\begin{") {
        return false;
    }

    let name_start = "\\begin{".len();
    let Some(name_end_rel) = trimmed[name_start..].find('}') else {
        return false;
    };
    let name_end = name_start.saturating_add(name_end_rel);
    if name_end <= name_start || name_end > trimmed.len() {
        return false;
    }
    let env_name = &trimmed[name_start..name_end];
    is_tex_math_environment_name(env_name)
}

fn split_tex_top_pieces(text: &str, rewrite_headings: bool) -> Vec<(String, bool)> {
    if text.is_empty() {
        return vec![(String::new(), false)];
    }

    // TeX 的 chunk 目标是“渲染出来的文本流”，而不是源码行。
    //
    // 这里先用 TexAdapter 找出“跨行的语法强约束片段”，将其作为临时的独立 piece，
    // 避免被“空行/段落分隔符”解析误伤（例如环境内部出现空行时，不应触发段落切分）。
    //
    // 后续在段落块级别，会把“数学块”尽量并回相邻段落，避免保护区把 chunk 切断。
    //
    // 但注意：`% ... EOL` 注释同样包含换行符；注释行经常用于“吞换行/控制空格”，
    // 不能把它们当作独立 block，否则会把同一段切碎。
    let regions = TexAdapter::split_regions(text, rewrite_headings);

    let mut pieces: Vec<(String, bool)> = Vec::new();
    let mut current = String::new();

    for region in regions.into_iter() {
        let is_multiline = region.body.contains('\n') || region.body.contains('\r');
        let is_comment = region.skip_rewrite && is_tex_comment_span(&region.body);
        let is_skip_block = region.skip_rewrite && is_multiline && !is_comment;

        if is_skip_block {
            if !current.is_empty() {
                pieces.push((std::mem::take(&mut current), false));
            }
            pieces.push((region.body, true));
            continue;
        }

        current.push_str(&region.body);
    }

    if !current.is_empty() {
        pieces.push((current, false));
    }

    if pieces.is_empty() {
        vec![(text.to_string(), false)]
    } else {
        pieces
    }
}

fn shift_one_leading_line_ending_to_previous_skip_piece(pieces: &mut Vec<(String, bool)>) {
    if pieces.len() < 2 {
        return;
    }

    for index in 1..pieces.len() {
        let (left, right) = pieces.split_at_mut(index);
        let (prev_body, prev_is_skip) = &mut left[index - 1];
        if !*prev_is_skip {
            continue;
        }

        let (curr_body, _) = &mut right[0];
        // 允许闭合行末存在尾随空格：把 `[\t ]*\r?\n` 一起挪回 skip piece，
        // 避免把“上一行的行末空白”误判成“新段落的空行”。
        let bytes = curr_body.as_bytes();
        let mut ws_len = 0usize;
        while ws_len < bytes.len() && (bytes[ws_len] == b' ' || bytes[ws_len] == b'\t') {
            ws_len = ws_len.saturating_add(1);
        }

        let line_ending_len = if ws_len < bytes.len() && bytes[ws_len] == b'\n' {
            1usize
        } else if ws_len < bytes.len() && bytes[ws_len] == b'\r' {
            if ws_len + 1 < bytes.len() && bytes[ws_len + 1] == b'\n' {
                2usize
            } else {
                1usize
            }
        } else {
            0usize
        };

        if line_ending_len > 0 {
            let cut = ws_len.saturating_add(line_ending_len);
            prev_body.push_str(&curr_body[..cut]);
            curr_body.replace_range(..cut, "");
        }
    }

    pieces.retain(|(body, _)| !body.is_empty());
}

fn coalesce_tex_math_skip_blocks(
    blocks: Vec<ParagraphBlock>,
    rewrite_headings: bool,
) -> Vec<ParagraphBlock> {
    if blocks.is_empty() {
        return blocks;
    }

    let mut iter = blocks.into_iter().peekable();
    let mut out: Vec<ParagraphBlock> = Vec::new();

    while let Some(block) = iter.next() {
        let is_math_block = !tex_body_has_editable_text(&block.body, rewrite_headings)
            && is_tex_math_skip_block_span(&block.body);

        if !is_math_block {
            out.push(block);
            continue;
        }

        if let Some(last) = out.last_mut() {
            // 仅在“同一段落内”合并：如果上一块已经进入 separator（空行/\par），
            // 说明出现了段落边界，不应跨段落合并。
            if last.separator_after.is_empty() {
                last.body.push_str(&block.body);
                last.separator_after.push_str(&block.separator_after);

                // 关键：数学块通常位于同一段落的中间（渲染文本上仍应连续）。
                // 但由于我们把“跨行 skip 区”临时拆成独立 block，可能出现：
                //   [正文] [数学块] [正文]
                // 这种“被保护区切断”的段落。这里把紧随其后的“正文续行块”并回去，
                // 让段落级 chunk 回到“连续可读”的状态。
                if let Some(next) = iter.peek() {
                    if !next.body.is_empty() {
                        let next_is_math =
                            !tex_body_has_editable_text(&next.body, rewrite_headings)
                                && is_tex_math_skip_block_span(&next.body);
                        let next_is_boundary = tex_block_first_content_line(&next.body)
                            .is_some_and(|line| {
                                is_tex_heading_command_span(line) || is_tex_begin_command_line(line)
                            });

                        if !next_is_math && !next_is_boundary {
                            if let Some(next) = iter.next() {
                                last.body.push_str(&next.body);
                                last.separator_after.push_str(&next.separator_after);
                            }
                        }
                    }
                }
                continue;
            }
        }

        // 尝试并到下一块：同样只在“无段落边界”的情况下做。
        let can_merge_next = block.separator_after.is_empty()
            && iter.peek().is_some_and(|next| {
                if next.body.is_empty() {
                    return false;
                }
                let next_is_math = !tex_body_has_editable_text(&next.body, rewrite_headings)
                    && is_tex_math_skip_block_span(&next.body);
                let next_is_boundary =
                    tex_block_first_content_line(&next.body).is_some_and(|line| {
                        is_tex_heading_command_span(line) || is_tex_begin_command_line(line)
                    });

                next_is_math || !next_is_boundary
            });
        if can_merge_next {
            if let Some(next) = iter.peek_mut() {
                let mut merged_prefix = block.body;
                merged_prefix.push_str(&block.separator_after);
                merged_prefix.push_str(&next.body);
                next.body = merged_prefix;
            }
            continue;
        }

        out.push(block);
    }

    out
}

fn strip_tex_comment_from_line(line: &str) -> &str {
    // TeX 注释语义存在 catcode 特例（例如 \verb|%| 中的 % 并不是注释）。
    // 这里复用 TexAdapter 的识别结果，只在确认是“真正注释片段”时才截断。
    if !line.contains('%') {
        return line;
    }

    let regions = TexAdapter::split_regions(line, true);
    let mut pos = 0usize;
    for region in regions.into_iter() {
        if region.skip_rewrite && is_tex_comment_span(&region.body) {
            if pos <= line.len() {
                return &line[..pos];
            }
            break;
        }
        pos = pos.saturating_add(region.body.len());
    }

    line
}

fn is_tex_begin_command_line(body: &str) -> bool {
    let trimmed = body.trim_start();
    if !trimmed.starts_with('\\') {
        return false;
    }
    let lowered = trimmed.to_ascii_lowercase();
    if !lowered.starts_with("\\begin") {
        return false;
    }
    let rest = &lowered["\\begin".len()..];
    rest.is_empty()
        || rest
            .chars()
            .next()
            .is_some_and(|ch| ch.is_whitespace() || ch == '{' || ch == '[')
}

fn is_tex_end_command_line(body: &str) -> bool {
    let trimmed = body.trim_start();
    if !trimmed.starts_with('\\') {
        return false;
    }
    let lowered = trimmed.to_ascii_lowercase();
    if !lowered.starts_with("\\end") {
        return false;
    }
    let rest = &lowered["\\end".len()..];
    rest.is_empty()
        || rest
            .chars()
            .next()
            .is_some_and(|ch| ch.is_whitespace() || ch == '{' || ch == '[')
}

fn is_tex_par_command_span(body: &str) -> bool {
    let trimmed = body.trim_start();
    if !trimmed.starts_with('\\') {
        return false;
    }

    let lowered = trimmed.to_ascii_lowercase();
    if !lowered.starts_with("\\par") {
        return false;
    }

    let rest = &lowered["\\par".len()..];
    if rest.is_empty() {
        return true;
    }
    let Some(first) = rest.chars().next() else {
        return true;
    };

    // 排除 \paragraph / \parbox 等更长命令
    if first.is_ascii_alphabetic() {
        return false;
    }

    true
}

fn split_tex_render_blocks_in_text_piece(text: &str) -> Vec<ParagraphBlock> {
    let bytes = text.as_bytes();
    let mut lines: Vec<(&str, &str)> = Vec::new();
    let mut start = 0usize;
    let mut index = 0usize;

    while index < bytes.len() {
        match bytes[index] {
            b'\n' => {
                let content = &text[start..index];
                let ending = &text[index..index + 1];
                lines.push((content, ending));
                index += 1;
                start = index;
            }
            b'\r' => {
                if index + 1 < bytes.len() && bytes[index + 1] == b'\n' {
                    let content = &text[start..index];
                    let ending = &text[index..index + 2];
                    lines.push((content, ending));
                    index += 2;
                    start = index;
                } else {
                    let content = &text[start..index];
                    let ending = &text[index..index + 1];
                    lines.push((content, ending));
                    index += 1;
                    start = index;
                }
            }
            _ => index += 1,
        }
    }

    if start < bytes.len() {
        lines.push((&text[start..], ""));
    } else if text.is_empty() {
        lines.push(("", ""));
    }

    let mut blocks: Vec<ParagraphBlock> = Vec::new();
    let mut current_body = String::new();
    let mut pending_sep = String::new();
    let mut in_sep = false;

    for (content, ending) in lines.into_iter() {
        let raw_line = format!("{content}{ending}");
        let stripped = strip_tex_comment_from_line(content);
        let trimmed = stripped.trim_start();
        // 注意：仅包含注释（例如 `% ...`）的行在 TeX 渲染里并不是“空行/段落分隔符”，
        // 它们更像是“隐形的空白控制符”。如果把它当作空行，会把同一段正文切碎，
        // 造成段落级分块过碎（你反馈的“连一句完整的话都不在同一块里”）。
        let is_comment_only_line =
            content.trim_start().starts_with('%') && stripped.trim().is_empty();
        let is_blank = stripped.trim().is_empty() && !is_comment_only_line;

        let is_par_line = is_tex_par_command_span(trimmed)
            && trimmed
                .get("\\par".len()..)
                .is_some_and(|rest| rest.trim().is_empty());

        let is_sep_line = is_blank || is_par_line;

        if in_sep {
            if is_sep_line {
                pending_sep.push_str(&raw_line);
                continue;
            }

            blocks.push(ParagraphBlock {
                body: std::mem::take(&mut current_body),
                separator_after: std::mem::take(&mut pending_sep),
            });
            in_sep = false;
        }

        if is_sep_line {
            pending_sep.push_str(&raw_line);
            in_sep = true;
            continue;
        }

        current_body.push_str(&raw_line);
    }

    blocks.push(ParagraphBlock {
        body: current_body,
        separator_after: pending_sep,
    });

    blocks
}

fn tex_body_has_editable_text(body: &str, rewrite_headings: bool) -> bool {
    let regions = TexAdapter::split_regions(body, rewrite_headings);
    regions
        .into_iter()
        .any(|region| !region.skip_rewrite && region.body.chars().any(|ch| !ch.is_whitespace()))
}

fn tex_block_is_single_structural_line(
    body: &str,
    rewrite_headings: bool,
    predicate: fn(&str) -> bool,
) -> bool {
    if body.is_empty() {
        return false;
    }

    // 只对“单行结构命令”做合并，避免误伤多行内容。
    let (trimmed, _) = super::split_trailing_whitespace(body);
    if trimmed.contains('\n') || trimmed.contains('\r') {
        return false;
    }

    let stripped = strip_tex_comment_from_line(&trimmed);
    if stripped.trim().is_empty() {
        return false;
    }

    if tex_body_has_editable_text(stripped, rewrite_headings) {
        return false;
    }

    predicate(stripped)
}

fn coalesce_tex_begin_end_blocks(
    blocks: Vec<ParagraphBlock>,
    rewrite_headings: bool,
) -> Vec<ParagraphBlock> {
    let mut out: Vec<ParagraphBlock> = Vec::new();
    let mut pending_prefix = String::new();

    for mut block in blocks.into_iter() {
        let is_begin = tex_block_is_single_structural_line(
            &block.body,
            rewrite_headings,
            is_tex_begin_command_line,
        );
        if is_begin {
            // `\\begin{...}` 在渲染中通常不可见，作为独立审阅单元没有意义。
            // 即使其后存在空行/\\par，也应尽量并入下一块，避免出现“孤儿 chunk”。
            pending_prefix.push_str(&block.body);
            pending_prefix.push_str(&block.separator_after);
            continue;
        }

        if !pending_prefix.is_empty() {
            let mut merged = std::mem::take(&mut pending_prefix);
            merged.push_str(&block.body);
            block.body = merged;
        }

        let is_end = tex_block_is_single_structural_line(
            &block.body,
            rewrite_headings,
            is_tex_end_command_line,
        );
        if is_end {
            if let Some(last) = out.last_mut() {
                // \end{...} 属于结构标记：应尽量黏到上一块，避免成为“孤儿审阅单元”。
                //
                // 即使上一块存在空白 separator（例如环境结束前空行），也应保持黏附。
                // 同时为了避免把这些结构/空白暴露给模型，这里把 \end 行放进上一块的 separator_after。
                last.separator_after.push_str(&block.body);
                last.separator_after.push_str(&block.separator_after);
                continue;
            }
        }

        out.push(block);
    }

    if !pending_prefix.is_empty() {
        out.push(ParagraphBlock {
            body: pending_prefix,
            separator_after: String::new(),
        });
    }

    out
}

fn tex_block_first_content_line(body: &str) -> Option<&str> {
    for raw_line in body.lines() {
        let stripped = strip_tex_comment_from_line(raw_line);
        if stripped.trim().is_empty() {
            continue;
        }
        let trimmed = stripped.trim_start();
        if is_tex_par_command_span(trimmed) {
            continue;
        }
        // 跳过最常见的“结构前缀”：\begin{...}
        if is_tex_begin_command_line(trimmed) {
            continue;
        }
        return Some(trimmed);
    }
    None
}

fn tex_block_is_heading_block(body: &str) -> bool {
    tex_block_first_content_line(body).is_some_and(|line| is_tex_heading_command_span(line))
}

fn tex_chars_with_editable_mask(text: &str, rewrite_headings: bool) -> (Vec<char>, Vec<bool>) {
    let regions = TexAdapter::split_regions(text, rewrite_headings);
    let mut chars: Vec<char> = Vec::new();
    let mut editable: Vec<bool> = Vec::new();

    for region in regions.into_iter() {
        let is_editable = !region.skip_rewrite;
        for ch in region.body.chars() {
            chars.push(ch);
            editable.push(is_editable);
        }
    }

    (chars, editable)
}

fn is_tex_char_escaped(chars: &[char], index: usize) -> bool {
    if index == 0 {
        return false;
    }
    let mut backslashes = 0usize;
    let mut pos = index;
    while pos > 0 {
        pos -= 1;
        if chars[pos] == '\\' {
            backslashes = backslashes.saturating_add(1);
        } else {
            break;
        }
    }
    backslashes % 2 == 1
}

fn segment_tex_by_boundary(
    text: &str,
    kind: BoundaryKind,
    max_chars: usize,
    rewrite_headings: bool,
) -> Vec<SegmentedChunk> {
    let (chars, editable) = tex_chars_with_editable_mask(text, rewrite_headings);
    let mut chunks = Vec::new();

    let mut current = String::new();
    let mut index = 0usize;
    let mut current_len = 0usize;
    let mut brace_depth: i32 = 0;
    let mut pending_boundary = false;
    let mut pending_max_cut = false;

    while index < chars.len() {
        let ch = chars[index];
        current.push(ch);
        current_len = current_len.saturating_add(1);

        // TeX 安全切分：避免切到未闭合的 `{...}` 里，否则会把命令参数拆坏。
        if ch == '{' && !is_tex_char_escaped(&chars, index) {
            brace_depth = brace_depth.saturating_add(1);
        } else if ch == '}' && !is_tex_char_escaped(&chars, index) {
            brace_depth = brace_depth.saturating_sub(1);
        }

        let is_boundary = if editable.get(index).copied().unwrap_or(false) {
            match kind {
                BoundaryKind::Sentence => is_sentence_boundary(&chars, index),
                BoundaryKind::Clause => is_clause_boundary(&chars, index),
            }
        } else {
            false
        };

        if is_boundary && brace_depth != 0 {
            pending_boundary = true;
        }

        let hit_max = max_chars > 0 && current_len >= max_chars;
        if hit_max && brace_depth != 0 {
            pending_max_cut = true;
        }

        let mut should_cut = false;

        if brace_depth == 0 {
            if pending_boundary {
                should_cut = true;
                pending_boundary = false;
            } else if is_boundary {
                should_cut = true;
            } else if pending_max_cut {
                should_cut = true;
                pending_max_cut = false;
            } else if hit_max {
                should_cut = true;
            }
        }

        if should_cut && !hit_max {
            // TeX 中也会出现连续的句末标点（例如 `？？` / `!!` / `...`），
            // 以及 `？”` 这类“标点 + 闭合符号”的组合。
            //
            // 如果只在第一个标点处切，会让后续标点变成“下一块的开头”，
            // 视觉上像被错误切碎。这里把紧邻的边界标点和闭合符号合并进同一块。
            while index + 1 < chars.len() {
                let next_index = index + 1;
                let next_ch = chars[next_index];
                let next_is_editable = editable.get(next_index).copied().unwrap_or(false);

                // 保守：不吞入不可改写字符，避免把受保护的 TeX 结构拆开。
                if !next_is_editable {
                    break;
                }

                let is_boundary_cluster = match kind {
                    BoundaryKind::Sentence => is_sentence_boundary(&chars, next_index),
                    BoundaryKind::Clause => is_clause_boundary(&chars, next_index),
                };

                if is_closing_punctuation(next_ch) || is_boundary_cluster {
                    index = next_index;
                    current.push(next_ch);
                    current_len = current_len.saturating_add(1);
                    continue;
                }

                break;
            }
        }

        if should_cut {
            let mut separator_after = String::new();
            let mut next = index + 1;
            while next < chars.len() && chars[next].is_whitespace() {
                separator_after.push(chars[next]);
                next += 1;
            }

            let (body, trailing_ws) = super::split_trailing_whitespace(&current);
            let mut merged_separator = trailing_ws;
            merged_separator.push_str(&separator_after);

            if body.is_empty() {
                super::append_separator_to_last(&mut chunks, merged_separator);
            } else {
                chunks.push(SegmentedChunk {
                    text: body,
                    separator_after: merged_separator,
                    skip_rewrite: false,
                });
            }

            current.clear();
            current_len = 0;
            brace_depth = 0;
            pending_boundary = false;
            pending_max_cut = false;
            index = next;
            continue;
        }

        index += 1;
    }

    if !current.is_empty() {
        let (body, trailing_ws) = super::split_trailing_whitespace(&current);
        if body.is_empty() {
            super::append_separator_to_last(&mut chunks, trailing_ws);
        } else {
            chunks.push(SegmentedChunk {
                text: body,
                separator_after: trailing_ws,
                skip_rewrite: false,
            });
        }
    }

    chunks
}

pub(super) fn segment_tex_text(
    text: &str,
    preset: ChunkPreset,
    rewrite_headings: bool,
) -> Vec<SegmentedChunk> {
    let mut pieces = split_tex_top_pieces(text, rewrite_headings);
    // TexAdapter 的数学块 `\\[ ... \\]` / `$$ ... $$` span 通常不包含闭合行末的换行符，
    // 会导致下一段 piece 以换行符开头，从而在“按渲染文本流”拆段时误判出一个空行块，
    // 进而让数学块把同一段切碎。
    //
    // 这里把紧跟在 multiline skip piece 后面的“一个行末换行符”挪回 skip piece，
    // 以保持行结构与原文一致，同时不改变整体拼回文本。
    shift_one_leading_line_ending_to_previous_skip_piece(&mut pieces);
    let mut blocks: Vec<ParagraphBlock> = Vec::new();

    for (body, is_skip_block) in pieces.into_iter() {
        if body.is_empty() {
            continue;
        }

        if is_skip_block {
            blocks.push(ParagraphBlock {
                body,
                separator_after: String::new(),
            });
            continue;
        }

        blocks.extend(split_tex_render_blocks_in_text_piece(&body));
    }

    let blocks = coalesce_tex_math_skip_blocks(blocks, rewrite_headings);
    let blocks = coalesce_tex_begin_end_blocks(blocks, rewrite_headings);

    let mut chunks: Vec<SegmentedChunk> = Vec::new();
    for block in blocks.into_iter() {
        if block.body.is_empty() {
            super::append_separator_to_last(&mut chunks, block.separator_after);
            continue;
        }

        // 纯结构/纯锁定块：直接跳过改写（例如 verbatim 环境、纯命令行等）。
        let has_editable = tex_body_has_editable_text(&block.body, rewrite_headings);
        let (body, trailing_ws) = super::split_trailing_whitespace(&block.body);
        let mut separator = trailing_ws;
        separator.push_str(&block.separator_after);

        if body.is_empty() {
            super::append_separator_to_last(&mut chunks, separator);
            continue;
        }

        // 标题块：无论 preset 如何，都保持整体原样（避免标题被切成多个审阅单元）。
        //
        // 关键点：当 rewrite_headings=false 时，仅标题命令本身会被 TexAdapter 锁定；
        // 若标题同行还带有正文（例如 `\\section{标题} 正文...`），正文仍应允许改写。
        if tex_block_is_heading_block(&body) {
            chunks.push(SegmentedChunk {
                text: body,
                separator_after: separator,
                skip_rewrite: !has_editable,
            });
            continue;
        }

        if !has_editable {
            chunks.push(SegmentedChunk {
                text: body,
                separator_after: separator,
                skip_rewrite: true,
            });
            continue;
        }

        match preset {
            ChunkPreset::Paragraph => {
                chunks.push(SegmentedChunk {
                    text: body,
                    separator_after: separator,
                    skip_rewrite: false,
                });
            }
            ChunkPreset::Sentence => {
                let mut pieces =
                    segment_tex_by_boundary(&body, BoundaryKind::Sentence, 0, rewrite_headings);
                super::append_separator_to_last(&mut pieces, separator);
                chunks.extend(pieces);
            }
            ChunkPreset::Clause => {
                let mut pieces =
                    segment_tex_by_boundary(&body, BoundaryKind::Clause, 0, rewrite_headings);
                super::append_separator_to_last(&mut pieces, separator);
                chunks.extend(pieces);
            }
        }
    }

    if chunks.is_empty() {
        vec![SegmentedChunk {
            text: text.to_string(),
            separator_after: String::new(),
            skip_rewrite: false,
        }]
    } else {
        chunks
    }
}
