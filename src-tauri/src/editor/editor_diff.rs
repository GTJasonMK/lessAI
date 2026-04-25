use serde::Serialize;

use crate::{
    models::{DiffResult, DiffType, DocumentSession},
    rewrite,
    rewrite_unit::rewrite_unit_text,
};

const INVALID_DIFF_PROJECTION_ERROR: &str = "编辑器 diff 投影失败：出现了非法字符边界。";

#[derive(Debug, Clone)]
struct EditorBaselineUnit {
    id: String,
    before_text: String,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EditorDiffStats {
    pub inserted: usize,
    pub deleted: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EditorDiffHunk {
    pub id: String,
    pub sequence: usize,
    pub rewrite_unit_id: String,
    pub diff: DiffResult,
    pub before_text: String,
    pub after_text: String,
    pub inserted_chars: usize,
    pub deleted_chars: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EditorDiffReport {
    pub baseline_text: String,
    pub stats: EditorDiffStats,
    pub hunks: Vec<EditorDiffHunk>,
    #[serde(default)]
    pub degraded_reason: Option<String>,
}

pub(crate) fn build_editor_diff_report(
    session: &DocumentSession,
    editor_text: &str,
) -> Result<EditorDiffReport, String> {
    let baseline_units = build_baseline_units(session)?;
    let baseline_text = baseline_units
        .iter()
        .map(|unit| unit.before_text.as_str())
        .collect::<String>();
    let normalized_editor_text = normalize_newlines(editor_text);
    let diff = rewrite::build_diff_result(&baseline_text, &normalized_editor_text);
    let hunks = project_editor_hunks(&baseline_units, &diff)?;
    let degraded_reason = diff
        .degraded_reason
        .clone()
        .or_else(|| first_degraded_reason(&hunks));

    Ok(EditorDiffReport {
        baseline_text,
        stats: diff_stats(&diff),
        hunks,
        degraded_reason,
    })
}

fn build_baseline_units(session: &DocumentSession) -> Result<Vec<EditorBaselineUnit>, String> {
    session
        .rewrite_units
        .iter()
        .map(|unit| {
            rewrite_unit_text(session, &unit.id).map(|before_text| EditorBaselineUnit {
                id: unit.id.clone(),
                before_text: normalize_newlines(&before_text),
            })
        })
        .collect()
}

fn project_editor_hunks(
    baseline_units: &[EditorBaselineUnit],
    diff: &DiffResult,
) -> Result<Vec<EditorDiffHunk>, String> {
    if baseline_units.is_empty() {
        return Ok(Vec::new());
    }

    let mut after_units = vec![String::new(); baseline_units.len()];
    let mut cursor_unit_index = 0usize;
    let mut cursor_offset_in_unit = 0usize;

    for span in &diff.spans {
        match span.r#type {
            DiffType::Unchanged => consume_before_text(
                baseline_units,
                &mut after_units,
                &mut cursor_unit_index,
                &mut cursor_offset_in_unit,
                &span.text,
                true,
            )?,
            DiffType::Delete => consume_before_text(
                baseline_units,
                &mut after_units,
                &mut cursor_unit_index,
                &mut cursor_offset_in_unit,
                &span.text,
                false,
            )?,
            DiffType::Insert => append_insert(
                baseline_units,
                &mut after_units,
                &mut cursor_unit_index,
                &mut cursor_offset_in_unit,
                &span.text,
            ),
        }
    }

    let mut hunks = Vec::new();
    for (index, baseline) in baseline_units.iter().enumerate() {
        let after_text = after_units[index].clone();
        if baseline.before_text == after_text {
            continue;
        }

        let diff = rewrite::build_diff_result(&baseline.before_text, &after_text);
        let stats = diff_stats(&diff);
        hunks.push(EditorDiffHunk {
            id: format!("rewrite-unit-{}", baseline.id),
            sequence: hunks.len() + 1,
            rewrite_unit_id: baseline.id.clone(),
            diff,
            before_text: baseline.before_text.clone(),
            after_text,
            inserted_chars: stats.inserted,
            deleted_chars: stats.deleted,
        });
    }

    Ok(hunks)
}

fn consume_before_text(
    baseline_units: &[EditorBaselineUnit],
    after_units: &mut [String],
    cursor_unit_index: &mut usize,
    cursor_offset_in_unit: &mut usize,
    text: &str,
    append_to_after: bool,
) -> Result<(), String> {
    let mut remaining = text;

    while !remaining.is_empty() {
        advance_unit_for_consumption(
            baseline_units,
            cursor_unit_index,
            cursor_offset_in_unit,
        );

        if *cursor_unit_index >= baseline_units.len() {
            if append_to_after {
                after_units[baseline_units.len() - 1].push_str(remaining);
            }
            return Ok(());
        }

        let unit_text = &baseline_units[*cursor_unit_index].before_text;
        let available = unit_text.len().saturating_sub(*cursor_offset_in_unit);
        if available == 0 {
            *cursor_unit_index += 1;
            *cursor_offset_in_unit = 0;
            continue;
        }

        let take = available.min(remaining.len());
        let (slice, rest) = split_at_byte(remaining, take)?;
        if append_to_after {
            after_units[*cursor_unit_index].push_str(slice);
        }
        *cursor_offset_in_unit += take;
        remaining = rest;
    }

    Ok(())
}

fn append_insert(
    baseline_units: &[EditorBaselineUnit],
    after_units: &mut [String],
    cursor_unit_index: &mut usize,
    cursor_offset_in_unit: &mut usize,
    text: &str,
) {
    advance_unit_for_consumption(
        baseline_units,
        cursor_unit_index,
        cursor_offset_in_unit,
    );

    if *cursor_unit_index >= baseline_units.len() {
        after_units[baseline_units.len() - 1].push_str(text);
        return;
    }

    if *cursor_offset_in_unit == 0 && *cursor_unit_index > 0 {
        let (leading, rest) = split_leading_whitespace_with_newline(text);
        if !leading.is_empty() {
            after_units[*cursor_unit_index - 1].push_str(leading);
        }
        if !rest.is_empty() {
            after_units[*cursor_unit_index].push_str(rest);
        }
        return;
    }

    after_units[*cursor_unit_index].push_str(text);
}

fn advance_unit_for_consumption(
    baseline_units: &[EditorBaselineUnit],
    cursor_unit_index: &mut usize,
    cursor_offset_in_unit: &mut usize,
) {
    while *cursor_unit_index < baseline_units.len()
        && *cursor_offset_in_unit == baseline_units[*cursor_unit_index].before_text.len()
    {
        *cursor_unit_index += 1;
        *cursor_offset_in_unit = 0;
    }
}

fn split_leading_whitespace_with_newline(text: &str) -> (&str, &str) {
    if text.is_empty() {
        return ("", "");
    }

    let mut split_index = 0usize;
    let mut has_newline = false;
    for (index, ch) in text.char_indices() {
        if ch == '\n' || ch == '\r' {
            has_newline = true;
            split_index = index + ch.len_utf8();
            continue;
        }
        if ch == ' ' || ch == '\t' {
            split_index = index + ch.len_utf8();
            continue;
        }
        break;
    }

    if !has_newline {
        return ("", text);
    }
    text.split_at(split_index)
}

fn split_at_byte(text: &str, byte_index: usize) -> Result<(&str, &str), String> {
    if !text.is_char_boundary(byte_index) {
        return Err(INVALID_DIFF_PROJECTION_ERROR.to_string());
    }
    Ok(text.split_at(byte_index))
}

fn normalize_newlines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn diff_stats(diff: &DiffResult) -> EditorDiffStats {
    let mut stats = EditorDiffStats::default();
    for span in &diff.spans {
        match span.r#type {
            DiffType::Insert => stats.inserted += count_non_whitespace_chars(&span.text),
            DiffType::Delete => stats.deleted += count_non_whitespace_chars(&span.text),
            DiffType::Unchanged => {}
        }
    }
    stats
}

fn count_non_whitespace_chars(text: &str) -> usize {
    text.chars().filter(|ch| !ch.is_whitespace()).count()
}

fn first_degraded_reason(hunks: &[EditorDiffHunk]) -> Option<String> {
    hunks.iter()
        .find_map(|hunk| hunk.diff.degraded_reason.clone())
}
