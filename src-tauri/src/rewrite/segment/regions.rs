use crate::adapters::TextRegion;
use crate::models::ChunkPreset;

use super::SegmentedChunk;

pub fn segment_regions(regions: Vec<TextRegion>, preset: ChunkPreset) -> Vec<SegmentedChunk> {
    let original = regions
        .iter()
        .map(|region| region.body.as_str())
        .collect::<String>();

    let mut chunks: Vec<SegmentedChunk> = Vec::new();
    for region in regions.into_iter() {
        if region.body.is_empty() {
            continue;
        }

        if region.skip_rewrite {
            append_raw_chunk(&mut chunks, &region.body, true);
            continue;
        }

        let mut pieces = super::plain::segment_plain_text(&region.body, preset);
        if !chunks.is_empty() && !pieces.is_empty() && pieces[0].text.is_empty() {
            let leading = pieces.remove(0).separator_after;
            if !leading.is_empty() {
                if let Some(last) = chunks.last_mut() {
                    last.separator_after.push_str(&leading);
                }
            }
        }
        chunks.extend(pieces);
    }

    if chunks.is_empty() {
        vec![SegmentedChunk {
            text: original,
            separator_after: String::new(),
            skip_rewrite: false,
        }]
    } else {
        chunks
    }
}

fn append_raw_chunk(chunks: &mut Vec<SegmentedChunk>, text: &str, skip_rewrite: bool) {
    let (body, trailing_ws) = super::split_trailing_whitespace(text);
    if body.is_empty() {
        super::append_separator_to_last(chunks, trailing_ws);
        return;
    }

    chunks.push(SegmentedChunk {
        text: body,
        separator_after: trailing_ws,
        skip_rewrite,
    });
}
