mod boundary;
mod markdown;
mod plain;
mod regions;
mod tex;

use crate::models::{ChunkPreset, DocumentFormat};

use super::SegmentedChunk;

#[derive(Debug, Clone)]
struct ParagraphBlock {
    body: String,
    separator_after: String,
}

pub fn segment_text(
    text: &str,
    preset: ChunkPreset,
    format: DocumentFormat,
    rewrite_headings: bool,
) -> Vec<SegmentedChunk> {
    match format {
        DocumentFormat::PlainText => plain::segment_plain_text(text, preset),
        DocumentFormat::Markdown => markdown::segment_markdown_text(text, preset, rewrite_headings),
        DocumentFormat::Tex => tex::segment_tex_text(text, preset, rewrite_headings),
    }
}

pub use regions::segment_regions;

fn split_trailing_whitespace(text: &str) -> (String, String) {
    let trimmed = text.trim_end_matches(|ch: char| ch.is_whitespace());
    let suffix = text[trimmed.len()..].to_string();
    (trimmed.to_string(), suffix)
}

fn append_separator_to_last(chunks: &mut Vec<SegmentedChunk>, separator: String) {
    if separator.is_empty() {
        return;
    }

    if let Some(last) = chunks.last_mut() {
        last.separator_after.push_str(&separator);
    } else {
        chunks.push(SegmentedChunk {
            text: String::new(),
            separator_after: separator,
            // 纯分隔符 chunk（例如文件头部的空行/空白行）。
            // 不应进入重写队列，否则会导致无意义调用或格式抖动风险。
            skip_rewrite: true,
        });
    }
}
