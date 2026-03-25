use crate::adapters::markdown::MarkdownAdapter;
use crate::models::ChunkPreset;

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

    super::segment_regions(regions, preset)
}
