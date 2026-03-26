use crate::adapters::markdown::MarkdownAdapter;
use crate::models::ChunkPreset;

use super::boundary::{
    is_clause_boundary, is_closing_punctuation, is_sentence_boundary, BoundaryKind,
};
use super::SegmentedChunk;

pub(super) fn segment_markdown_text(
    text: &str,
    preset: ChunkPreset,
    rewrite_headings: bool,
) -> Vec<SegmentedChunk> {
    let regions = MarkdownAdapter::split_regions(text, rewrite_headings);

    if regions.len() == 1 && !regions[0].skip_rewrite {
        return super::plain::segment_plain_text(text, preset);
    }

    // Markdown 的 chunk 目标是“审阅/改写的最小单元”（句/段等），而不是语法片段。
    //
    // 因此：
    // - 行内保护区（例如 `$...$` 公式、`inline code`、链接语法、强调标记等）不应把 chunk 切碎；
    // - 但跨行的高风险片段（例如 fenced code block、table、front matter、整行 HTML 注释等）
    //   仍应作为独立 chunk 跳过改写（避免破坏结构）。
    //
    // 这里采用“可编辑 mask 分块”：
    // - 将 MarkdownAdapter 输出的 regions 串接为字符流；
    // - protected（skip_rewrite=true）的字符在边界检测时不参与断句；
    // - 仅当遇到“跨行 skip region”时强制切断，作为独立的 skip chunk。
    let mut out: Vec<SegmentedChunk> = Vec::new();
    let mut buffer_chars: Vec<char> = Vec::new();
    let mut buffer_editable: Vec<bool> = Vec::new();
    let mut at_line_start = true;

    let flush_buffer =
        |out: &mut Vec<SegmentedChunk>, chars: &mut Vec<char>, editable: &mut Vec<bool>| {
            if chars.is_empty() {
                return;
            }

            let blocks = split_markdown_paragraph_blocks(chars, editable);
            for block in blocks.into_iter() {
                segment_markdown_block(out, block, preset);
            }

            chars.clear();
            editable.clear();
        };

    for region in regions.into_iter() {
        if region.body.is_empty() {
            continue;
        }

        if region.skip_rewrite && (region.body.contains('\n') || region.body.contains('\r')) {
            // 特例：数学块 `$$ ... $$`（由 MarkdownAdapter 作为跨行 skip region 输出）
            // 应尽量保留在同一 chunk 内，避免“保护区把 chunk 切碎”导致段落读起来断断续续。
            //
            // 注意：段落边界仍然由空行/列表项等规则决定；这里只是避免“仅因为数学块跨行”就强制切断。
            let is_math_block = is_markdown_math_block_region(&region.body);

            // 保护区是否要“切断 chunk”：
            // - 多行块（fenced code block / table / front matter 等）=> 必须切断，独立 skip chunk；
            // - 单行块（reference definition / horizontal rule / indented code / heading 等整行结构）
            //   => 仅在确认是“块级结构行”时才切断；
            // - 行内保护区（例如 `$...$`、`inline code`、强调标记）就算被拼上了行尾换行符，也不应切断 chunk。
            let (trimmed, _) = super::split_trailing_whitespace(&region.body);
            let has_internal_linebreak = trimmed.contains('\n') || trimmed.contains('\r');
            let is_block_skip = has_internal_linebreak
                || (at_line_start && is_markdown_block_level_skip_line(&trimmed));

            if is_block_skip && !is_math_block {
                flush_buffer(&mut out, &mut buffer_chars, &mut buffer_editable);
                append_raw_skip_chunk(&mut out, &region.body);
                for ch in region.body.chars() {
                    if ch == '\n' || ch == '\r' {
                        at_line_start = true;
                    } else if at_line_start {
                        at_line_start = false;
                    }
                }
                continue;
            }
        }

        let editable_flag = !region.skip_rewrite;
        for ch in region.body.chars() {
            buffer_chars.push(ch);
            buffer_editable.push(editable_flag);
            if ch == '\n' || ch == '\r' {
                at_line_start = true;
            } else if at_line_start {
                at_line_start = false;
            }
        }
    }

    flush_buffer(&mut out, &mut buffer_chars, &mut buffer_editable);

    if out.is_empty() {
        vec![SegmentedChunk {
            text: text.to_string(),
            separator_after: String::new(),
            skip_rewrite: false,
        }]
    } else {
        out
    }
}

#[derive(Debug, Clone)]
struct MaskedParagraphBlock {
    body: Vec<char>,
    editable: Vec<bool>,
    separator_after: String,
}

fn append_raw_skip_chunk(chunks: &mut Vec<SegmentedChunk>, text: &str) {
    let (body, trailing_ws) = super::split_trailing_whitespace(text);
    if body.is_empty() {
        super::append_separator_to_last(chunks, trailing_ws);
        return;
    }
    chunks.push(SegmentedChunk {
        text: body,
        separator_after: trailing_ws,
        skip_rewrite: true,
    });
}

fn is_markdown_block_level_skip_line(line: &str) -> bool {
    let trimmed = line.trim_start_matches('\u{feff}').trim_start();
    if trimmed.is_empty() {
        return false;
    }

    // fenced code marker（即使这里只是单行，也更像块级结构）
    if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
        return true;
    }

    // 数学块分隔符：单独一行 `$$`
    if trimmed == "$$" {
        return true;
    }

    // 缩进代码块（4 空格 / tab）通常作为块级结构。
    if trimmed.starts_with('\t') || line.starts_with("    ") {
        return true;
    }

    // ATX 标题：### ...
    if trimmed.starts_with('#') {
        return true;
    }

    // HTML-like 行：`<tag ...>` / `</tag>` / `<!DOCTYPE ...>` 等。
    if trimmed.starts_with('<') {
        return true;
    }

    // 水平线：--- / *** / ___（允许空白）
    let mut chars = trimmed.chars().filter(|ch| !ch.is_whitespace());
    let Some(first) = chars.next() else {
        return false;
    };
    if matches!(first, '-' | '*' | '_') {
        let mut count = 1usize;
        for ch in chars {
            if ch == first {
                count = count.saturating_add(1);
                continue;
            }
            // 出现其它字符则不是水平线
            count = 0;
            break;
        }
        if count >= 3 {
            return true;
        }
    }

    // reference definition / footnote definition：`[id]: ...` / `[^1]: ...`
    if trimmed.starts_with('[') {
        let bytes = trimmed.as_bytes();
        let mut p = 1usize;
        while p < bytes.len() {
            if bytes[p] == b']' {
                break;
            }
            p += 1;
        }
        if p + 1 < bytes.len() && bytes[p] == b']' && bytes[p + 1] == b':' {
            return true;
        }
    }

    false
}

fn is_markdown_math_block_region(body: &str) -> bool {
    if body.is_empty() {
        return false;
    }

    let mut lines = body.lines();
    let Some(first) = lines.next() else {
        return false;
    };

    let Some(last) = body.lines().rev().find(|line| !line.trim().is_empty()) else {
        return false;
    };

    first.trim() == "$$" && last.trim() == "$$"
}

fn split_markdown_paragraph_blocks(chars: &[char], editable: &[bool]) -> Vec<MaskedParagraphBlock> {
    let mut lines: Vec<(Vec<char>, Vec<bool>, Vec<char>, Vec<bool>)> = Vec::new();
    let mut content: Vec<char> = Vec::new();
    let mut content_mask: Vec<bool> = Vec::new();

    let mut index = 0usize;
    while index < chars.len() {
        let ch = chars[index];
        if ch == '\n' {
            lines.push((
                std::mem::take(&mut content),
                std::mem::take(&mut content_mask),
                vec!['\n'],
                vec![editable[index]],
            ));
            index += 1;
            continue;
        }
        if ch == '\r' {
            if index + 1 < chars.len() && chars[index + 1] == '\n' {
                lines.push((
                    std::mem::take(&mut content),
                    std::mem::take(&mut content_mask),
                    vec!['\r', '\n'],
                    vec![editable[index], editable[index + 1]],
                ));
                index += 2;
                continue;
            }
            lines.push((
                std::mem::take(&mut content),
                std::mem::take(&mut content_mask),
                vec!['\r'],
                vec![editable[index]],
            ));
            index += 1;
            continue;
        }

        content.push(ch);
        content_mask.push(editable[index]);
        index += 1;
    }

    if !content.is_empty() || !content_mask.is_empty() {
        lines.push((
            std::mem::take(&mut content),
            std::mem::take(&mut content_mask),
            Vec::new(),
            Vec::new(),
        ));
    } else if chars.is_empty() {
        lines.push((Vec::new(), Vec::new(), Vec::new(), Vec::new()));
    }

    let mut blocks: Vec<MaskedParagraphBlock> = Vec::new();
    let mut current_body: Vec<char> = Vec::new();
    let mut current_editable: Vec<bool> = Vec::new();
    let mut current_sep = String::new();
    let mut in_sep = false;

    for (line_content, line_mask, line_ending, ending_mask) in lines.into_iter() {
        let is_blank = line_content.iter().all(|ch| ch.is_whitespace());

        let mut raw_line: Vec<char> = Vec::with_capacity(line_content.len() + line_ending.len());
        raw_line.extend_from_slice(&line_content);
        raw_line.extend_from_slice(&line_ending);

        let mut raw_mask: Vec<bool> = Vec::with_capacity(line_mask.len() + ending_mask.len());
        raw_mask.extend_from_slice(&line_mask);
        raw_mask.extend_from_slice(&ending_mask);

        if in_sep {
            if is_blank {
                current_sep.push_str(&raw_line.iter().collect::<String>());
                continue;
            }

            blocks.push(MaskedParagraphBlock {
                body: std::mem::take(&mut current_body),
                editable: std::mem::take(&mut current_editable),
                separator_after: std::mem::take(&mut current_sep),
            });
            in_sep = false;
        }

        if is_blank {
            current_sep.push_str(&raw_line.iter().collect::<String>());
            in_sep = true;
            continue;
        }

        current_body.extend_from_slice(&raw_line);
        current_editable.extend_from_slice(&raw_mask);
    }

    blocks.push(MaskedParagraphBlock {
        body: current_body,
        editable: current_editable,
        separator_after: current_sep,
    });

    blocks
}

fn split_trailing_whitespace_with_mask(
    chars: &[char],
    editable: &[bool],
) -> (Vec<char>, Vec<bool>, String) {
    if chars.is_empty() {
        return (Vec::new(), Vec::new(), String::new());
    }

    let mut end = chars.len();
    while end > 0 && chars[end - 1].is_whitespace() {
        end -= 1;
    }

    let trailing = chars[end..].iter().collect::<String>();
    (chars[..end].to_vec(), editable[..end].to_vec(), trailing)
}

fn has_editable_text(chars: &[char], editable: &[bool]) -> bool {
    chars
        .iter()
        .zip(editable.iter())
        .any(|(ch, is_editable)| *is_editable && !ch.is_whitespace())
}

fn segment_markdown_block(
    out: &mut Vec<SegmentedChunk>,
    block: MaskedParagraphBlock,
    preset: ChunkPreset,
) {
    let (body_chars, body_editable, trailing_ws) =
        split_trailing_whitespace_with_mask(&block.body, &block.editable);
    let mut paragraph_separator = trailing_ws;
    paragraph_separator.push_str(&block.separator_after);

    if body_chars.is_empty() {
        super::append_separator_to_last(out, paragraph_separator);
        return;
    }

    match preset {
        ChunkPreset::Paragraph => {
            let skip_rewrite = !has_editable_text(&body_chars, &body_editable);
            out.push(SegmentedChunk {
                text: body_chars.iter().collect::<String>(),
                separator_after: paragraph_separator,
                skip_rewrite,
            });
        }
        ChunkPreset::Sentence => {
            let mut pieces =
                segment_markdown_by_boundary(&body_chars, &body_editable, BoundaryKind::Sentence);
            super::append_separator_to_last(&mut pieces, paragraph_separator);
            out.extend(pieces);
        }
        ChunkPreset::Clause => {
            let mut pieces =
                segment_markdown_by_boundary(&body_chars, &body_editable, BoundaryKind::Clause);
            super::append_separator_to_last(&mut pieces, paragraph_separator);
            out.extend(pieces);
        }
    }
}

fn segment_markdown_by_boundary(
    chars: &[char],
    editable: &[bool],
    kind: BoundaryKind,
) -> Vec<SegmentedChunk> {
    let mut chunks: Vec<SegmentedChunk> = Vec::new();
    let mut current_chars: Vec<char> = Vec::new();
    let mut current_editable: Vec<bool> = Vec::new();
    let mut index = 0usize;

    while index < chars.len() {
        let ch = chars[index];
        current_chars.push(ch);
        current_editable.push(editable.get(index).copied().unwrap_or(false));

        let should_cut = if editable.get(index).copied().unwrap_or(false) {
            match kind {
                BoundaryKind::Sentence => is_sentence_boundary(chars, index),
                BoundaryKind::Clause => is_clause_boundary(chars, index),
            }
        } else {
            false
        };

        if should_cut {
            // Markdown 下也常见连续句末标点：`？？` / `!!` / `...`。
            // 这里同 plain 分块：把紧邻的连续边界标点和闭合符号吞到同一块里，
            // 避免出现“块首是一个孤零零的 `？`”的体验。
            while index + 1 < chars.len() {
                let next_index = index + 1;
                let next_ch = chars[next_index];
                let next_is_editable = editable.get(next_index).copied().unwrap_or(false);

                // 保守策略：不吞入非 editable 字符，避免把 protected region 切碎。
                if !next_is_editable {
                    break;
                }

                let is_boundary_cluster = match kind {
                    BoundaryKind::Sentence => is_sentence_boundary(chars, next_index),
                    BoundaryKind::Clause => is_clause_boundary(chars, next_index),
                };

                if is_closing_punctuation(next_ch) || is_boundary_cluster {
                    index = next_index;
                    current_chars.push(next_ch);
                    current_editable.push(next_is_editable);
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

            let (body_chars, body_editable, trailing_ws) =
                split_trailing_whitespace_with_mask(&current_chars, &current_editable);
            let mut merged_separator = trailing_ws;
            merged_separator.push_str(&separator_after);

            if body_chars.is_empty() {
                super::append_separator_to_last(&mut chunks, merged_separator);
            } else {
                let skip_rewrite = !has_editable_text(&body_chars, &body_editable);
                chunks.push(SegmentedChunk {
                    text: body_chars.iter().collect::<String>(),
                    separator_after: merged_separator,
                    skip_rewrite,
                });
            }

            current_chars.clear();
            current_editable.clear();
            index = next;
            continue;
        }

        index += 1;
    }

    if !current_chars.is_empty() {
        let (body_chars, body_editable, trailing_ws) =
            split_trailing_whitespace_with_mask(&current_chars, &current_editable);
        if body_chars.is_empty() {
            super::append_separator_to_last(&mut chunks, trailing_ws);
        } else {
            let skip_rewrite = !has_editable_text(&body_chars, &body_editable);
            chunks.push(SegmentedChunk {
                text: body_chars.iter().collect::<String>(),
                separator_after: trailing_ws,
                skip_rewrite,
            });
        }
    }

    chunks
}
