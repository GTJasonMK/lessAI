use crate::adapters::TextRegion;

use super::stream::{SegmentRegion, SegmentRegionRole};

pub(super) fn build_markdown_stream(regions: Vec<TextRegion>) -> Vec<SegmentRegion> {
    let mut at_line_start = true;
    regions
        .into_iter()
        .filter(|region| !region.body.is_empty())
        .map(|region| {
            let role = classify_markdown_region(&region.body, region.skip_rewrite, at_line_start);
            advance_line_state(&region.body, &mut at_line_start);
            SegmentRegion {
                body: region.body,
                skip_rewrite: region.skip_rewrite,
                presentation: region.presentation,
                role,
            }
        })
        .collect::<Vec<_>>()
}

fn classify_markdown_region(
    body: &str,
    skip_rewrite: bool,
    at_line_start: bool,
) -> SegmentRegionRole {
    if !skip_rewrite {
        return SegmentRegionRole::Flow;
    }

    let (trimmed, _) = super::split_trailing_whitespace(body);
    if is_markdown_math_block_region(body) {
        return SegmentRegionRole::Flow;
    }
    if trimmed.contains('\n') || trimmed.contains('\r') {
        return SegmentRegionRole::Isolated;
    }
    if at_line_start && is_markdown_block_level_skip_line(&trimmed) {
        return SegmentRegionRole::Isolated;
    }
    SegmentRegionRole::Flow
}

fn advance_line_state(body: &str, at_line_start: &mut bool) {
    for ch in body.chars() {
        if ch == '\n' || ch == '\r' {
            *at_line_start = true;
            continue;
        }
        if *at_line_start {
            *at_line_start = false;
        }
    }
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
