use crate::adapters::TextRegion;

use super::inline_emphasis::{find_matching_emphasis, parse_emphasis_delimiter_run};
use super::inline_spans::find_markdown_protected_spans;
use super::syntax::markdown_line_prefix_len;

#[derive(Debug, Clone, Copy)]
pub(super) struct LineSlice<'a> {
    pub line: &'a str,
    pub full: &'a str,
}

pub(super) fn split_lines_with_endings(text: &str) -> Vec<LineSlice<'_>> {
    let bytes = text.as_bytes();
    let mut lines: Vec<LineSlice<'_>> = Vec::new();
    let mut start = 0usize;
    let mut index = 0usize;

    while index < bytes.len() {
        match bytes[index] {
            b'\n' => {
                lines.push(LineSlice {
                    line: &text[start..index],
                    full: &text[start..index + 1],
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
                lines.push(LineSlice {
                    line: &text[start..index],
                    full: &text[start..end],
                });
                index = end;
                start = index;
            }
            _ => index += 1,
        }
    }

    if start < bytes.len() {
        lines.push(LineSlice {
            line: &text[start..bytes.len()],
            full: &text[start..bytes.len()],
        });
    } else if text.is_empty() {
        lines.push(LineSlice { line: "", full: "" });
    }

    lines
}

pub(super) fn process_markdown_line(out: &mut Vec<TextRegion>, line: &str, ending: &str) {
    let prefix_len = markdown_line_prefix_len(line);
    let (prefix, core) = if prefix_len > 0 && prefix_len <= line.len() {
        (&line[..prefix_len], &line[prefix_len..])
    } else {
        ("", line)
    };

    if !prefix.is_empty() {
        push_text_region(out, TextRegion::syntax_token(prefix));
    }

    let spans = find_markdown_protected_spans(core);
    if spans.is_empty() {
        push_rewriteable_markdown_text(out, core);
        append_line_ending(out, ending);
        return;
    }

    let mut cursor = 0usize;
    for (start, end) in spans {
        if start > cursor {
            push_rewriteable_markdown_text(out, &core[cursor..start]);
        }
        push_text_region(out, protected_markdown_region(&core[start..end]));
        cursor = end;
    }
    if cursor < core.len() {
        push_rewriteable_markdown_text(out, &core[cursor..]);
    }

    append_line_ending(out, ending);
}

pub(super) fn push_text_region(regions: &mut Vec<TextRegion>, region: TextRegion) {
    if region.body.is_empty() {
        return;
    }

    if let Some(last) = regions.last_mut() {
        if last.skip_rewrite == region.skip_rewrite
            && last.role == region.role
            && last.split_mode == region.split_mode
            && last.presentation == region.presentation
        {
            last.body.push_str(&region.body);
            return;
        }
    }

    regions.push(region);
}

fn append_line_ending(out: &mut Vec<TextRegion>, ending: &str) {
    if ending.is_empty() {
        return;
    }
    if let Some(last) = out.last_mut() {
        last.body.push_str(ending);
    } else {
        out.push(TextRegion::syntax_token(ending));
    }
}

fn push_rewriteable_markdown_text(out: &mut Vec<TextRegion>, text: &str) {
    if text.is_empty() {
        return;
    }

    let bytes = text.as_bytes();
    let mut cursor = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        let Some(run) = parse_emphasis_delimiter_run(text, index) else {
            index += 1;
            continue;
        };
        if !run.can_open {
            index = run.end;
            continue;
        }
        let Some((open_len, close_start, close_len)) = find_matching_emphasis(text, run) else {
            index = run.end;
            continue;
        };

        if run.start > cursor {
            push_editable_markdown_text(out, &text[cursor..run.start]);
        }

        push_locked_markdown_text(out, &text[run.start..run.start + open_len]);

        let inner_start = run.start + open_len;
        let inner_end = close_start;
        if inner_end > inner_start {
            push_rewriteable_markdown_text(out, &text[inner_start..inner_end]);
        }

        push_locked_markdown_text(out, &text[close_start..close_start + close_len]);

        cursor = close_start + close_len;
        index = cursor;
    }

    if cursor < text.len() {
        push_editable_markdown_text(out, &text[cursor..]);
    }
}

fn push_editable_markdown_text(out: &mut Vec<TextRegion>, text: &str) {
    push_text_region(
        out,
        TextRegion::editable(text),
    );
}

fn push_locked_markdown_text(out: &mut Vec<TextRegion>, text: &str) {
    push_text_region(out, TextRegion::syntax_token(text));
}

fn protected_markdown_region(text: &str) -> TextRegion {
    if text.starts_with('<') {
        if text.starts_with("<!--")
            || text[1..].starts_with("http://")
            || text[1..].starts_with("https://")
            || text[1..].starts_with("mailto:")
        {
            return TextRegion::inline_object(text);
        }
        return TextRegion::syntax_token(text);
    }

    if text.starts_with('`')
        || text.starts_with('$')
        || text.starts_with("![")
        || text.starts_with('[')
        || text.starts_with("http://")
        || text.starts_with("https://")
        || text.starts_with("www.")
    {
        return TextRegion::inline_object(text);
    }

    TextRegion::locked_text(text)
}
