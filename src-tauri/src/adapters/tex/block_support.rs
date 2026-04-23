use super::environments::{
    begin_environment_name, end_environment_name, is_math_environment_name, is_raw_environment_name,
};
use super::scan::find_command_span_end;

#[derive(Clone, Copy)]
pub(super) struct IndexedLine<'a> {
    pub line: &'a str,
    pub start: usize,
    pub end: usize,
}

pub(super) struct BlankLines {
    pub text: String,
    pub next_index: usize,
}

const LIST_ENVIRONMENTS: &[&str] = &["itemize", "enumerate", "description"];
const HEADING_COMMANDS: &[&str] = &[
    "\\part",
    "\\chapter",
    "\\section",
    "\\subsection",
    "\\subsubsection",
    "\\paragraph",
    "\\subparagraph",
];

pub(super) fn find_locked_block_end(
    lines: &[IndexedLine<'_>],
    start: usize,
) -> Option<(usize, &'static str)> {
    let line = lines[start].line.trim_start();

    if line.starts_with('%') {
        let mut end = start + 1;
        while end < lines.len() && lines[end].line.trim_start().starts_with('%') {
            end += 1;
        }
        return Some((end, "locked_block"));
    }

    if line == "$$" {
        return Some((find_math_delimiter_end(lines, start, "$$"), "math_block"));
    }

    if line == "\\[" {
        return Some((find_math_delimiter_end(lines, start, "\\]"), "math_block"));
    }

    let name = begin_environment_name(line)?;
    let kind = if is_raw_environment_name(name) {
        "locked_block"
    } else if is_math_environment_name(name) {
        "math_block"
    } else {
        return None;
    };

    let mut end = start + 1;
    while end < lines.len() {
        if end_environment_name(lines[end].line.trim_start()).is_some_and(|candidate| candidate == name)
        {
            return Some((end + 1, kind));
        }
        end += 1;
    }
    Some((lines.len(), kind))
}

pub(super) fn classify_text_block_kind(
    text: &str,
    fallback: Option<&'static str>,
) -> &'static str {
    let trimmed = text.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('%') {
        return "locked_block";
    }
    if trimmed.starts_with("$$") || trimmed.starts_with("\\[") {
        return "math_block";
    }
    if begin_environment_name(trimmed).is_some() || end_environment_name(trimmed).is_some() {
        return "environment_block";
    }
    if trimmed.starts_with('\\') {
        return fallback.unwrap_or("command_block");
    }
    fallback.unwrap_or("paragraph")
}

pub(super) fn is_heading_command_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    HEADING_COMMANDS
        .iter()
        .any(|command| matches_tex_command(trimmed, command))
}

pub(super) fn heading_command_block_end(
    text: &str,
    lines: &[IndexedLine<'_>],
    start: usize,
) -> Option<usize> {
    let line = lines.get(start)?.line;
    if !is_heading_command_line(line) {
        return None;
    }

    let command_start = lines[start].start + line.len().saturating_sub(line.trim_start().len());
    let span_end = find_command_span_end(text, command_start)?;
    Some(line_index_after_offset(lines, span_end))
}

pub(super) fn is_item_line(line: &str) -> bool {
    matches_tex_command(line.trim_start(), "\\item")
}

pub(super) fn is_list_environment_begin(line: &str) -> bool {
    begin_environment_name(line)
        .map(|name| LIST_ENVIRONMENTS.contains(&name))
        .unwrap_or(false)
}

pub(super) fn is_list_environment_end(line: &str) -> bool {
    end_environment_name(line)
        .map(|name| LIST_ENVIRONMENTS.contains(&name))
        .unwrap_or(false)
}

pub(super) fn collect_blank_lines(
    text: &str,
    lines: &[IndexedLine<'_>],
    start: usize,
) -> BlankLines {
    let mut index = start;
    while index < lines.len() && lines[index].line.trim().is_empty() {
        index += 1;
    }
    BlankLines {
        text: if start < index {
            slice_text(text, lines, start, index)
        } else {
            String::new()
        },
        next_index: index,
    }
}

pub(super) fn slice_text(
    text: &str,
    lines: &[IndexedLine<'_>],
    start: usize,
    end: usize,
) -> String {
    text[lines[start].start..lines[end - 1].end].to_string()
}

pub(super) fn split_lines_with_offsets(text: &str) -> Vec<IndexedLine<'_>> {
    let bytes = text.as_bytes();
    let mut lines = Vec::new();
    let mut start = 0usize;
    let mut index = 0usize;

    while index < bytes.len() {
        match bytes[index] {
            b'\n' => {
                lines.push(IndexedLine {
                    line: &text[start..index],
                    start,
                    end: index + 1,
                });
                index += 1;
                start = index;
            }
            b'\r' => {
                let end = if index + 1 < bytes.len() && bytes[index + 1] == b'\n' {
                    index + 2
                } else {
                    index + 1
                };
                lines.push(IndexedLine {
                    line: &text[start..index],
                    start,
                    end,
                });
                index = end;
                start = index;
            }
            _ => index += 1,
        }
    }

    if start < bytes.len() {
        lines.push(IndexedLine {
            line: &text[start..bytes.len()],
            start,
            end: bytes.len(),
        });
    } else if text.is_empty() {
        lines.push(IndexedLine {
            line: "",
            start: 0,
            end: 0,
        });
    }

    lines
}

fn matches_tex_command(line: &str, command: &str) -> bool {
    if !line.starts_with(command) {
        return false;
    }
    let rest = &line[command.len()..];
    rest.is_empty()
        || rest.starts_with('*')
        || rest.starts_with('{')
        || rest.starts_with('[')
        || rest.chars().next().is_some_and(|ch| ch.is_whitespace())
}

fn find_math_delimiter_end(lines: &[IndexedLine<'_>], start: usize, delimiter: &str) -> usize {
    let mut end = start + 1;
    while end < lines.len() {
        if lines[end].line.trim() == delimiter {
            return end + 1;
        }
        end += 1;
    }
    lines.len()
}

fn line_index_after_offset(lines: &[IndexedLine<'_>], offset: usize) -> usize {
    for (index, line) in lines.iter().enumerate() {
        if offset <= line.end {
            return index + 1;
        }
    }
    lines.len()
}
