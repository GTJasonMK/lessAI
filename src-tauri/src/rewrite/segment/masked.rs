use crate::models::{ChunkPresentation, ChunkPreset};

use super::blocks::{split_masked_paragraph_blocks, MaskedParagraphBlock};
use super::boundary::{
    is_clause_boundary, is_closing_punctuation, is_sentence_boundary, BoundaryKind,
};
use super::guards::BoundaryGuard;
use super::{append_separator_to_last, split_trailing_whitespace, SegmentedChunk};

pub(super) fn append_segmented_text<G: BoundaryGuard>(
    chunks: &mut Vec<SegmentedChunk>,
    text: &str,
    preset: ChunkPreset,
    presentation: Option<ChunkPresentation>,
) {
    let chars: Vec<char> = text.chars().collect();
    let editable = vec![true; chars.len()];
    append_segmented_masked_text::<G>(chunks, &chars, &editable, preset, presentation);
}

pub(super) fn append_segmented_masked_text<G: BoundaryGuard>(
    chunks: &mut Vec<SegmentedChunk>,
    chars: &[char],
    editable: &[bool],
    preset: ChunkPreset,
    presentation: Option<ChunkPresentation>,
) {
    let blocks = split_masked_paragraph_blocks(chars, editable);
    for block in blocks {
        append_segmented_block::<G>(chunks, block, preset, presentation.clone());
    }
}

fn append_segmented_block<G: BoundaryGuard>(
    chunks: &mut Vec<SegmentedChunk>,
    block: MaskedParagraphBlock,
    preset: ChunkPreset,
    presentation: Option<ChunkPresentation>,
) {
    let (body_chars, body_editable, trailing_ws) =
        split_trailing_whitespace_with_mask(&block.body, &block.editable);
    let paragraph_separator = format!("{trailing_ws}{}", block.separator_after);

    if body_chars.is_empty() {
        append_separator_to_last(chunks, paragraph_separator);
        return;
    }

    match preset {
        ChunkPreset::Paragraph => append_paragraph_chunk(
            chunks,
            &body_chars,
            &body_editable,
            paragraph_separator,
            presentation,
        ),
        ChunkPreset::Sentence => append_boundary_chunks::<G>(
            chunks,
            &body_chars,
            &body_editable,
            BoundaryKind::Sentence,
            paragraph_separator,
            presentation,
        ),
        ChunkPreset::Clause => append_boundary_chunks::<G>(
            chunks,
            &body_chars,
            &body_editable,
            BoundaryKind::Clause,
            paragraph_separator,
            presentation,
        ),
    }
}

fn append_paragraph_chunk(
    chunks: &mut Vec<SegmentedChunk>,
    body_chars: &[char],
    body_editable: &[bool],
    separator_after: String,
    presentation: Option<ChunkPresentation>,
) {
    let skip_rewrite = !has_editable_text(body_chars, body_editable);
    chunks.push(SegmentedChunk {
        text: body_chars.iter().collect::<String>(),
        separator_after,
        skip_rewrite,
        presentation,
    });
}

fn append_boundary_chunks<G: BoundaryGuard>(
    chunks: &mut Vec<SegmentedChunk>,
    chars: &[char],
    editable: &[bool],
    kind: BoundaryKind,
    paragraph_separator: String,
    presentation: Option<ChunkPresentation>,
) {
    let mut pieces = segment_masked_by_boundary::<G>(chars, editable, kind, presentation.clone());
    append_separator_to_last(&mut pieces, paragraph_separator);
    chunks.extend(pieces);
}

fn segment_masked_by_boundary<G: BoundaryGuard>(
    chars: &[char],
    editable: &[bool],
    kind: BoundaryKind,
    presentation: Option<ChunkPresentation>,
) -> Vec<SegmentedChunk> {
    let mut chunks = Vec::new();
    let mut current_chars = Vec::new();
    let mut current_editable = Vec::new();
    let mut guard = G::default();
    let mut index = 0usize;

    while index < chars.len() {
        current_chars.push(chars[index]);
        current_editable.push(editable.get(index).copied().unwrap_or(false));
        guard.observe_char(chars, index);

        let is_editable = editable.get(index).copied().unwrap_or(false);
        let hit_boundary = is_editable && matches_boundary(chars, index, kind);
        if !guard.should_cut(hit_boundary) {
            index += 1;
            continue;
        }

        index = extend_boundary_cluster(
            chars,
            editable,
            kind,
            index,
            &mut current_chars,
            &mut current_editable,
        );
        let next = push_current_masked_chunk(
            &mut chunks,
            chars,
            &current_chars,
            &current_editable,
            index,
            presentation.clone(),
        );
        current_chars.clear();
        current_editable.clear();
        guard.reset_after_cut();
        index = next;
    }

    push_remainder_chunk(&mut chunks, &current_chars, &current_editable, presentation);
    chunks
}

fn matches_boundary(chars: &[char], index: usize, kind: BoundaryKind) -> bool {
    match kind {
        BoundaryKind::Sentence => is_sentence_boundary(chars, index),
        BoundaryKind::Clause => is_clause_boundary(chars, index),
    }
}

fn extend_boundary_cluster(
    chars: &[char],
    editable: &[bool],
    kind: BoundaryKind,
    mut index: usize,
    current_chars: &mut Vec<char>,
    current_editable: &mut Vec<bool>,
) -> usize {
    while index + 1 < chars.len() {
        let next_index = index + 1;
        if !editable.get(next_index).copied().unwrap_or(false) {
            break;
        }

        let next = chars[next_index];
        if !is_closing_punctuation(next) && !matches_boundary(chars, next_index, kind) {
            break;
        }

        current_chars.push(next);
        current_editable.push(true);
        index = next_index;
    }
    index
}

fn push_current_masked_chunk(
    chunks: &mut Vec<SegmentedChunk>,
    chars: &[char],
    current_chars: &[char],
    current_editable: &[bool],
    index: usize,
    presentation: Option<ChunkPresentation>,
) -> usize {
    let (separator_after, next) = collect_separator_after(chars, index);
    let (body_chars, body_editable, trailing_ws) =
        split_trailing_whitespace_with_mask(current_chars, current_editable);
    let merged_separator = format!("{trailing_ws}{separator_after}");
    push_masked_chunk(
        chunks,
        &body_chars,
        &body_editable,
        merged_separator,
        presentation,
    );
    next
}

fn collect_separator_after(chars: &[char], index: usize) -> (String, usize) {
    let mut separator_after = String::new();
    let mut next = index + 1;
    while next < chars.len() && chars[next].is_whitespace() {
        separator_after.push(chars[next]);
        next += 1;
    }
    (separator_after, next)
}

fn push_remainder_chunk(
    chunks: &mut Vec<SegmentedChunk>,
    current_chars: &[char],
    current_editable: &[bool],
    presentation: Option<ChunkPresentation>,
) {
    let (body_chars, body_editable, trailing_ws) =
        split_trailing_whitespace_with_mask(current_chars, current_editable);
    push_masked_chunk(
        chunks,
        &body_chars,
        &body_editable,
        trailing_ws,
        presentation,
    );
}

fn push_masked_chunk(
    chunks: &mut Vec<SegmentedChunk>,
    body_chars: &[char],
    body_editable: &[bool],
    separator_after: String,
    presentation: Option<ChunkPresentation>,
) {
    if body_chars.is_empty() {
        append_separator_to_last(chunks, separator_after);
        return;
    }

    chunks.push(SegmentedChunk {
        text: body_chars.iter().collect::<String>(),
        separator_after,
        skip_rewrite: !has_editable_text(body_chars, body_editable),
        presentation,
    });
}

fn split_trailing_whitespace_with_mask(
    chars: &[char],
    editable: &[bool],
) -> (Vec<char>, Vec<bool>, String) {
    let text = chars.iter().collect::<String>();
    let (trimmed, trailing_ws) = split_trailing_whitespace(&text);
    let trimmed_len = trimmed.chars().count();
    (
        chars[..trimmed_len].to_vec(),
        editable[..trimmed_len].to_vec(),
        trailing_ws,
    )
}

fn has_editable_text(chars: &[char], editable: &[bool]) -> bool {
    chars
        .iter()
        .zip(editable.iter())
        .any(|(ch, is_editable)| *is_editable && !ch.is_whitespace())
}
