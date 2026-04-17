use super::super::text::{
    collapse_line_breaks_to_spaces, enforce_line_skeleton, normalize_line_endings_to_lf,
    split_line_skeleton, strip_redundant_prefix, trim_ascii_spaces_tabs_start,
};
use super::validate::validate_rewrite_output;

pub(super) fn finalize_plain_candidate(
    source_text: &str,
    candidate_text: &str,
) -> Result<String, String> {
    let candidate = if source_text.contains('\n') || source_text.contains('\r') {
        finalize_multiline_candidate(source_text, candidate_text)?
    } else {
        finalize_singleline_candidate(source_text, candidate_text)
    };

    validate_rewrite_output(source_text, &candidate)?;
    Ok(candidate)
}

fn finalize_multiline_candidate(source_text: &str, candidate_text: &str) -> Result<String, String> {
    let source_lines = split_lines_keep_empty(source_text);
    if source_lines.iter().all(|line| line.trim().is_empty()) {
        return Ok(source_text.to_string());
    }

    let expected = source_lines.len().max(1);
    let rewritten_lines =
        if let Some(lines) = try_parse_multiline_rewrite_response(candidate_text, expected) {
            if !blank_pattern_matches(&source_lines, &lines) {
                return Err("模型输出未保持原始空行结构。".to_string());
            }
            lines
        } else {
            let candidate_lines = split_lines_keep_empty(candidate_text);
            let numbering_changed = candidate_lines
                .iter()
                .any(|line| trim_ascii_spaces_tabs_start(line).starts_with("@@@"));

            if candidate_lines.len() != expected
                || !blank_pattern_matches(&source_lines, &candidate_lines)
                || numbering_changed
            {
                return Err("模型输出未按要求保持逐行结构。".to_string());
            }
            candidate_lines
        };

    let mut enforced = Vec::with_capacity(source_lines.len());
    for (source, candidate) in source_lines.iter().zip(rewritten_lines.iter()) {
        enforced.push(enforce_line_skeleton(source, candidate));
    }

    Ok(enforced.join("\n"))
}

fn finalize_singleline_candidate(source_text: &str, candidate_text: &str) -> String {
    let (prefix, core, suffix) = split_line_skeleton(source_text);
    if core.trim().is_empty() {
        return source_text.to_string();
    }

    let rewritten = collapse_line_breaks_to_spaces(candidate_text);
    let body = strip_redundant_prefix(&rewritten, &prefix);
    if body.trim().is_empty() {
        return source_text.to_string();
    }

    format!("{prefix}{body}{suffix}")
}

pub(super) fn split_lines_keep_empty(text: &str) -> Vec<String> {
    normalize_line_endings_to_lf(text)
        .split('\n')
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
}

pub(super) fn blank_pattern_matches(source_lines: &[String], candidate_lines: &[String]) -> bool {
    if source_lines.len() != candidate_lines.len() {
        return false;
    }

    source_lines
        .iter()
        .zip(candidate_lines.iter())
        .all(|(source, candidate)| source.trim().is_empty() == candidate.trim().is_empty())
}

pub(super) fn try_parse_multiline_rewrite_response(
    output: &str,
    expected_lines: usize,
) -> Option<Vec<String>> {
    let normalized = normalize_line_endings_to_lf(output);
    let mut collected: Vec<Option<String>> = vec![None; expected_lines];

    for raw_line in normalized.split('\n') {
        if !raw_line.starts_with("@@@") {
            continue;
        }

        let rest = &raw_line["@@@".len()..];
        let mut digits_end = 0usize;
        for (offset, ch) in rest.char_indices() {
            if ch.is_ascii_digit() {
                digits_end = offset + ch.len_utf8();
            } else {
                break;
            }
        }
        if digits_end == 0 {
            continue;
        }

        let num_str = &rest[..digits_end];
        let Ok(number) = num_str.parse::<usize>() else {
            continue;
        };
        if number == 0 || number > expected_lines {
            continue;
        }

        let after_digits = &rest[digits_end..];
        if !after_digits.starts_with("@@@") {
            continue;
        }

        if collected[number - 1].is_some() {
            return None;
        }
        collected[number - 1] = Some(after_digits["@@@".len()..].to_string());
    }

    if collected.iter().all(|value| value.is_some()) {
        Some(
            collected
                .into_iter()
                .map(|value| value.unwrap_or_default())
                .collect::<Vec<_>>(),
        )
    } else {
        None
    }
}
