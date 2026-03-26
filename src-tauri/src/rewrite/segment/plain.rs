use crate::models::ChunkPreset;

use super::boundary::{
    is_clause_boundary, is_closing_punctuation, is_sentence_boundary, BoundaryKind,
};
use super::{ParagraphBlock, SegmentedChunk};

pub(super) fn segment_plain_text(text: &str, preset: ChunkPreset) -> Vec<SegmentedChunk> {
    // 切块目标：
    // - 给 agent/LLM 一个稳定的“工作单元”
    // - 同时保证：把 chunks 拼回去后，原文格式不发生变化（空格/换行/空行都保留）
    //
    // 粒度：
    // - Clause：一小句（逗号/分号等）
    // - Sentence：一整句（句号/问号/感叹号等）
    // - Paragraph：一段话（空行分段）
    //
    // 注意：分块策略在“第一次 AI 优化之前”就必须确定并保持稳定；
    // 这里不做“按长度自动降级拆分”（例如段落过长就拆成句/小句），
    // 超长块导致模型调用失败的风险应由 UI 在开始优化时提示用户自行决策。
    let blocks = split_paragraph_blocks(text);

    let mut chunks: Vec<SegmentedChunk> = Vec::new();

    for block in blocks.into_iter() {
        let (body, trailing_ws) = super::split_trailing_whitespace(&block.body);
        let mut paragraph_separator = trailing_ws;
        paragraph_separator.push_str(&block.separator_after);

        if body.is_empty() {
            super::append_separator_to_last(&mut chunks, paragraph_separator);
            continue;
        }

        match preset {
            ChunkPreset::Paragraph => {
                chunks.push(SegmentedChunk {
                    text: body,
                    separator_after: paragraph_separator,
                    skip_rewrite: false,
                });
            }
            ChunkPreset::Sentence => {
                let mut pieces = segment_by_boundary(&body, BoundaryKind::Sentence);
                super::append_separator_to_last(&mut pieces, paragraph_separator);
                chunks.extend(pieces);
            }
            ChunkPreset::Clause => {
                let mut pieces = segment_by_boundary(&body, BoundaryKind::Clause);
                super::append_separator_to_last(&mut pieces, paragraph_separator);
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

fn split_paragraph_blocks(text: &str) -> Vec<ParagraphBlock> {
    let bytes = text.as_bytes();
    let mut lines: Vec<(String, String)> = Vec::new();
    let mut start = 0usize;
    let mut index = 0usize;

    while index < bytes.len() {
        match bytes[index] {
            b'\n' => {
                let content = &text[start..index];
                lines.push((content.to_string(), "\n".to_string()));
                index += 1;
                start = index;
            }
            b'\r' => {
                if index + 1 < bytes.len() && bytes[index + 1] == b'\n' {
                    let content = &text[start..index];
                    lines.push((content.to_string(), "\r\n".to_string()));
                    index += 2;
                    start = index;
                } else {
                    let content = &text[start..index];
                    lines.push((content.to_string(), "\r".to_string()));
                    index += 1;
                    start = index;
                }
            }
            _ => index += 1,
        }
    }

    if start < bytes.len() {
        lines.push((text[start..].to_string(), String::new()));
    } else if text.is_empty() {
        lines.push((String::new(), String::new()));
    }

    let mut blocks = Vec::new();
    let mut current_body = String::new();
    let mut current_sep = String::new();
    let mut in_sep = false;

    for (content, ending) in lines.into_iter() {
        let line = format!("{content}{ending}");
        let is_blank = content.trim().is_empty();

        if in_sep {
            if is_blank {
                current_sep.push_str(&line);
            } else {
                blocks.push(ParagraphBlock {
                    body: current_body,
                    separator_after: current_sep,
                });
                current_body = line;
                current_sep = String::new();
                in_sep = false;
            }
            continue;
        }

        if is_blank {
            current_sep.push_str(&line);
            in_sep = true;
        } else {
            current_body.push_str(&line);
        }
    }

    blocks.push(ParagraphBlock {
        body: current_body,
        separator_after: current_sep,
    });

    blocks
}

fn segment_by_boundary(text: &str, kind: BoundaryKind) -> Vec<SegmentedChunk> {
    let chars: Vec<char> = text.chars().collect();
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut index = 0usize;

    while index < chars.len() {
        let ch = chars[index];
        current.push(ch);

        let should_cut = match kind {
            BoundaryKind::Sentence => is_sentence_boundary(&chars, index),
            BoundaryKind::Clause => is_clause_boundary(&chars, index),
        };
        if should_cut {
            // 句末常见写法：`？？` / `!!` / `...` 等。
            // 如果只在第一个标点处截断，会导致第二个标点变成“下一块的开头”，
            // 审阅体验很割裂（甚至出现只包含一个 `？` 的 chunk）。
            while index + 1 < chars.len() {
                let next_index = index + 1;
                let next_ch = chars[next_index];

                let is_boundary_cluster = match kind {
                    BoundaryKind::Sentence => is_sentence_boundary(&chars, next_index),
                    BoundaryKind::Clause => is_clause_boundary(&chars, next_index),
                };
                if is_closing_punctuation(next_ch) || is_boundary_cluster {
                    index = next_index;
                    current.push(next_ch);
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
