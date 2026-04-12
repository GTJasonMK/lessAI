use crate::adapters::TextRegion;

use super::stream::{SegmentRegion, SegmentRegionRole};

pub(super) fn build_tex_stream(regions: Vec<TextRegion>) -> Vec<SegmentRegion> {
    regions
        .into_iter()
        .filter(|region| !region.body.is_empty())
        .map(|region| {
            let role = classify_tex_region(&region.body, region.skip_rewrite);
            SegmentRegion {
                body: region.body,
                skip_rewrite: region.skip_rewrite,
                presentation: region.presentation,
                role,
            }
        })
        .collect::<Vec<_>>()
}

fn classify_tex_region(body: &str, skip_rewrite: bool) -> SegmentRegionRole {
    if !skip_rewrite {
        return SegmentRegionRole::Flow;
    }

    let (trimmed, _) = super::split_trailing_whitespace(body);
    if is_tex_par_separator(&trimmed) {
        return SegmentRegionRole::Separator;
    }
    if is_tex_comment_span(body) || is_tex_math_skip_block_span(body) {
        return SegmentRegionRole::Flow;
    }
    if trimmed.contains('\n') || trimmed.contains('\r') {
        return SegmentRegionRole::Isolated;
    }
    SegmentRegionRole::Flow
}

fn is_tex_comment_span(body: &str) -> bool {
    body.trim_start().starts_with('%')
}

fn is_tex_par_separator(body: &str) -> bool {
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
    if first.is_ascii_alphabetic() {
        return false;
    }
    rest.trim().is_empty()
}

fn is_tex_math_skip_block_span(body: &str) -> bool {
    let trimmed = body.trim_start();
    if trimmed.starts_with("$$") || trimmed.starts_with("\\[") {
        return true;
    }
    parse_begin_environment_name(trimmed).is_some_and(is_tex_math_environment_name)
}

fn parse_begin_environment_name(body: &str) -> Option<&str> {
    let trimmed = body.trim_start();
    if !trimmed.starts_with("\\begin{") {
        return None;
    }

    let start = "\\begin{".len();
    let end = trimmed[start..].find('}')?;
    let name_end = start.saturating_add(end);
    if name_end <= start || name_end > trimmed.len() {
        return None;
    }
    Some(&trimmed[start..name_end])
}

fn is_tex_math_environment_name(name: &str) -> bool {
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
