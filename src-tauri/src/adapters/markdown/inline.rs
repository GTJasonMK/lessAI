use crate::{
    adapters::TextRegion,
    text_boundaries::split_text_and_trailing_separator,
    textual_template::models::{TextRegionSplitMode, TextTemplateRegion},
};

use super::inline_lines::{
    process_markdown_line, push_text_region, split_lines_with_endings,
};
use super::inline_scans::find_matching_bracket;
use super::inline_spans::find_markdown_link_end;
use super::syntax::markdown_line_prefix_len;

pub(super) fn build_regions(
    block_anchor: &str,
    block_text: &str,
    block_kind: &str,
    rewrite_headings: bool,
) -> Vec<TextTemplateRegion> {
    let regions = build_text_regions(block_text, block_kind, rewrite_headings);

    regions
        .into_iter()
        .enumerate()
        .map(|(region_index, region)| build_region(block_anchor, region_index, region))
        .collect()
}

pub(super) fn build_text_regions(
    block_text: &str,
    block_kind: &str,
    rewrite_headings: bool,
) -> Vec<TextRegion> {
    expand_locked_regions(parse_block_regions_for_kind(
        block_text,
        block_kind,
        rewrite_headings,
    ))
}

fn parse_block_regions_for_kind(
    text: &str,
    block_kind: &str,
    rewrite_headings: bool,
) -> Vec<TextRegion> {
    if text.is_empty() {
        return Vec::new();
    }
    if block_kind == "locked_block" || block_kind == "blank" {
        return vec![TextRegion::locked_text(text)];
    }
    if block_kind == "heading" && !rewrite_headings {
        return vec![TextRegion::locked_text(text)];
    }

    let lines = split_lines_with_endings(text);
    let mut out = Vec::new();
    for slice in lines {
        if slice.full.is_empty() {
            continue;
        }
        let ending = &slice.full[slice.line.len()..];
        process_markdown_line(&mut out, slice.line, ending);
    }
    out
}

fn build_region(block_anchor: &str, region_index: usize, region: TextRegion) -> TextTemplateRegion {
    let (text, separator_after) = split_text_and_trailing_separator(&region.body);

    TextTemplateRegion {
        anchor: format!("{block_anchor}:r{region_index}"),
        text,
        editable: !region.skip_rewrite,
        role: region.role,
        presentation: region.presentation,
        split_mode: region.split_mode,
        separator_after,
    }
}

fn expand_locked_regions(regions: Vec<TextRegion>) -> Vec<TextRegion> {
    let mut out = Vec::new();
    for region in regions {
        if !region.skip_rewrite {
            push_region_without_merging(&mut out, region);
            continue;
        }

        if let Some(expanded) = split_locked_link_region(&region.body) {
            for item in expanded {
                push_region_without_merging(&mut out, item);
            }
            continue;
        }

        push_region_without_merging(&mut out, region);
    }
    out
}

fn push_region_without_merging(regions: &mut Vec<TextRegion>, region: TextRegion) {
    if region.body.is_empty() {
        return;
    }
    regions.push(region);
}

fn split_locked_link_region(text: &str) -> Option<Vec<TextRegion>> {
    let (body, separator_after) = split_text_and_trailing_separator(text);
    if body.is_empty() {
        return None;
    }

    let mut out = Vec::new();
    let mut core = body.as_str();
    let prefix_len = markdown_line_prefix_len(core);
    if prefix_len > 0 && prefix_len < core.len() {
        let candidate = &core[prefix_len..];
        if candidate.starts_with('[') && !candidate.starts_with("![") {
            out.push(TextRegion::syntax_token(&core[..prefix_len]));
            core = candidate;
        }
    }

    if core.starts_with("![") || !core.starts_with('[') {
        return None;
    }
    if core.starts_with("[^")
        || core.starts_with("[@")
        || core.starts_with("[-@")
        || !has_parenthesized_link_target(core)?
        || find_markdown_link_end(core, 0)? != core.len()
    {
        return None;
    }

    let close = find_matching_bracket(core, 0)?;
    let target_start = find_link_target_start(core, close)?;
    let target_end = find_markdown_link_end(core, 0)?.saturating_sub(1);
    let label_start = 1usize;
    let label_end = close.saturating_sub(1);
    if label_end <= label_start || target_end < target_start {
        return None;
    }

    out.push(TextRegion::syntax_token(&core[..label_start]));

    let label = &core[label_start..label_end];
    for region in mark_editable_regions_atomic(parse_block_regions_for_kind(
        label,
        "paragraph",
        true,
    )) {
        push_text_region(&mut out, region);
    }

    out.push(TextRegion::syntax_token(&core[label_end..target_start + 1]));
    if target_end > target_start {
        out.push(TextRegion::inline_object(&core[target_start + 1..target_end]));
    }

    let mut closing = TextRegion::syntax_token(&core[target_end..]);
    closing.body.push_str(&separator_after);
    out.push(closing);
    Some(out)
}

fn has_parenthesized_link_target(text: &str) -> Option<bool> {
    let close = find_matching_bracket(text, 0)?;
    find_link_target_start(text, close).map(|_| true)
}

fn find_link_target_start(text: &str, close: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut pos = close;
    while pos < bytes.len() && matches!(bytes[pos], b' ' | b'\t') {
        pos += 1;
    }
    (pos < bytes.len() && bytes[pos] == b'(').then_some(pos)
}

fn mark_editable_regions_atomic(regions: Vec<TextRegion>) -> Vec<TextRegion> {
    regions
        .into_iter()
        .map(|region| {
            if region.skip_rewrite {
                region
            } else {
                region.with_split_mode(TextRegionSplitMode::Atomic)
            }
        })
        .collect()
}
