mod diff;
mod llm;
mod segment;
mod text;
mod types;

pub use diff::build_diff;
pub use llm::{build_client, rewrite_chunk, rewrite_chunk_with_client, test_provider};
pub use segment::{segment_regions, segment_text};
pub use text::{
    collapse_line_breaks_to_spaces, convert_line_endings, detect_line_ending,
    has_trailing_spaces_per_line, normalize_text, strip_trailing_spaces_per_line, LineEnding,
};
pub use types::SegmentedChunk;

#[cfg(test)]
mod tests;
