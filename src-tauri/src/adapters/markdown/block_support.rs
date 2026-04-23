use super::syntax::{
    detect_fence_marker, is_atx_heading_line, is_html_like_line, is_horizontal_rule_line,
    is_indented_code_line, is_list_or_quote_line, is_markdown_table_delimiter,
    is_math_block_delimiter_line, is_reference_definition_line, is_setext_underline_line,
    is_yaml_front_matter_close, is_yaml_front_matter_open,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MarkdownBlock {
    pub kind: &'static str,
    pub text: String,
}

#[derive(Clone, Copy)]
pub(super) struct IndexedLine<'a> {
    pub line: &'a str,
    pub start: usize,
    pub end: usize,
}

const MAX_FRONT_MATTER_LINES: usize = 200;

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

pub(super) fn find_yaml_front_matter_range(lines: &[IndexedLine<'_>]) -> Option<(usize, usize)> {
    let mut index = 0usize;
    while index < lines.len() && lines[index].line.trim().is_empty() {
        index += 1;
    }
    if index >= lines.len() || !is_yaml_front_matter_open(lines[index].line) {
        return None;
    }

    let start = index;
    index += 1;
    while index < lines.len() && index <= start + MAX_FRONT_MATTER_LINES {
        if is_yaml_front_matter_close(lines[index].line) {
            return Some((start, index));
        }
        index += 1;
    }
    None
}

pub(super) fn starts_standalone_markdown_block(lines: &[IndexedLine<'_>], index: usize) -> bool {
    let line = lines[index].line;
    if line.trim().is_empty() {
        return true;
    }
    detect_fence_marker(line).is_some()
        || is_math_block_delimiter_line(line)
        || is_table_start(lines, index)
        || is_atx_heading_line(line)
        || is_reference_definition_line(line)
        || is_html_like_line(line)
        || is_horizontal_rule_line(line)
        || is_indented_code_line(line)
        || is_list_or_quote_line(line)
        || (index + 1 < lines.len()
            && !line.trim().is_empty()
            && is_setext_underline_line(lines[index + 1].line))
}

pub(super) fn continues_list_or_quote_block(
    kind: &str,
    first_line: &str,
    next_line: &str,
) -> bool {
    if kind == "quote" {
        return next_line.trim_start().starts_with('>');
    }
    if !is_list_or_quote_line(next_line) {
        return true;
    }
    leading_indent_width(next_line) > leading_indent_width(first_line)
}

pub(super) fn push_block_with_trailing_blanks(
    blocks: &mut Vec<MarkdownBlock>,
    text: &str,
    lines: &[IndexedLine<'_>],
    start: usize,
    end: usize,
    kind: &'static str,
) -> usize {
    let next = extend_through_trailing_blank_lines(lines, end);
    push_block(blocks, text, lines, start, next, kind)
}

pub(super) fn push_block(
    blocks: &mut Vec<MarkdownBlock>,
    text: &str,
    lines: &[IndexedLine<'_>],
    start: usize,
    end: usize,
    kind: &'static str,
) -> usize {
    push_exact_block(blocks, text, lines, start, end, kind)
}

pub(super) fn is_table_start(lines: &[IndexedLine<'_>], index: usize) -> bool {
    index + 1 < lines.len()
        && !lines[index].line.trim().is_empty()
        && lines[index].line.contains('|')
        && is_markdown_table_delimiter(lines[index + 1].line)
}

pub(super) fn is_table_row(line: &str) -> bool {
    !line.trim().is_empty() && line.contains('|')
}

fn push_exact_block(
    blocks: &mut Vec<MarkdownBlock>,
    text: &str,
    lines: &[IndexedLine<'_>],
    start: usize,
    end: usize,
    kind: &'static str,
) -> usize {
    blocks.push(MarkdownBlock {
        kind,
        text: text[lines[start].start..lines[end - 1].end].to_string(),
    });
    end
}

fn extend_through_trailing_blank_lines(lines: &[IndexedLine<'_>], mut index: usize) -> usize {
    while index < lines.len() && lines[index].line.trim().is_empty() {
        index += 1;
    }
    index
}

fn leading_indent_width(line: &str) -> usize {
    line.chars()
        .take_while(|ch| ch.is_whitespace())
        .map(|ch| if ch == '\t' { 4 } else { 1 })
        .sum()
}
