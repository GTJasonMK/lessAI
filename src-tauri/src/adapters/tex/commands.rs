use crate::{
    adapters::TextRegion,
    text_boundaries::split_text_and_trailing_separator,
    textual_template::models::{TextRegionSplitMode, TextTemplateRegion},
};

use super::scan::{
    consume_whitespace, find_closing_double_dollar, find_closing_single_dollar,
    find_command_span_end, find_inline_delimited_command_end, find_inline_verb_end,
    find_line_end, find_skip_environment_span, find_substring, is_escaped, parse_brace_group,
    parse_bracket_group,
};

const HEADING_COMMANDS: &[&str] = &[
    "section", "subsection", "subsubsection", "paragraph", "subparagraph", "chapter", "part",
    "title", "subtitle", "caption",
];
const TEXT_COMMANDS: &[&str] = &[
    "footnote", "emph", "textbf", "textit", "underline", "textrm", "textsf", "textsc",
];
const LINK_TEXT_COMMANDS: &[&str] = &["href"];

pub(super) fn build_regions(block_anchor: &str, text: &str, rewrite_headings: bool) -> Vec<TextTemplateRegion> {
    parse_regions(text, rewrite_headings)
        .into_iter()
        .enumerate()
        .map(|(region_index, region)| build_region(block_anchor, region_index, region))
        .collect()
}

pub(super) fn parse_regions(text: &str, rewrite_headings: bool) -> Vec<TextRegion> {
    if text.is_empty() {
        return vec![TextRegion::editable(String::new())];
    }

    let bytes = text.as_bytes();
    let mut regions: Vec<TextRegion> = Vec::new();
    let mut last = 0usize;
    let mut index = 0usize;

    while index < bytes.len() {
        let step = text[index..]
            .chars()
            .next()
            .map(char::len_utf8)
            .unwrap_or(1);

        if bytes[index] == b'%' && !is_escaped(text, index) {
            push_region(&mut regions, TextRegion::editable(&text[last..index]));
            let end = find_line_end(text, index);
            push_region(&mut regions, TextRegion::locked_text(&text[index..end]));
            index = end;
            last = end;
            continue;
        }

        if text[index..].starts_with("$$") && !is_escaped(text, index) {
            push_region(&mut regions, TextRegion::editable(&text[last..index]));
            let end = find_closing_double_dollar(text, index + 2).unwrap_or(text.len());
            push_region(&mut regions, TextRegion::inline_object(&text[index..end]));
            index = end;
            last = end;
            continue;
        }

        if bytes[index] == b'$'
            && !is_escaped(text, index)
            && !text[index..].starts_with("$$")
            && find_closing_single_dollar(text, index + 1).is_some()
        {
            push_region(&mut regions, TextRegion::editable(&text[last..index]));
            let end = find_closing_single_dollar(text, index + 1).unwrap_or(text.len());
            push_region(&mut regions, TextRegion::inline_object(&text[index..end]));
            index = end;
            last = end;
            continue;
        }

        if bytes[index] == b'\\' {
            if text[index..].starts_with("\\(") && !is_escaped(text, index) {
                if let Some(end) = find_substring(text, index + 2, "\\)") {
                    push_region(&mut regions, TextRegion::editable(&text[last..index]));
                    push_region(&mut regions, TextRegion::inline_object(&text[index..end]));
                    index = end;
                    last = end;
                    continue;
                }
            }

            if text[index..].starts_with("\\[") && !is_escaped(text, index) {
                if let Some(end) = find_substring(text, index + 2, "\\]") {
                    push_region(&mut regions, TextRegion::editable(&text[last..index]));
                    push_region(&mut regions, TextRegion::inline_object(&text[index..end]));
                    index = end;
                    last = end;
                    continue;
                }
            }

            if let Some((span_start, span_end)) = find_skip_environment_span(text, index) {
                push_region(&mut regions, TextRegion::editable(&text[last..span_start]));
                push_region(
                    &mut regions,
                    TextRegion::inline_object(&text[span_start..span_end]),
                );
                index = span_end;
                last = span_end;
                continue;
            }

            if let Some(end) = find_inline_verb_end(text, index) {
                push_region(&mut regions, TextRegion::editable(&text[last..index]));
                push_region(&mut regions, TextRegion::inline_object(&text[index..end]));
                index = end;
                last = end;
                continue;
            }

            if let Some(end) = find_inline_delimited_command_end(text, index, "\\lstinline") {
                push_region(&mut regions, TextRegion::editable(&text[last..index]));
                push_region(&mut regions, TextRegion::inline_object(&text[index..end]));
                index = end;
                last = end;
                continue;
            }

            if let Some(end) = find_inline_delimited_command_end(text, index, "\\path") {
                push_region(&mut regions, TextRegion::editable(&text[last..index]));
                push_region(&mut regions, TextRegion::inline_object(&text[index..end]));
                index = end;
                last = end;
                continue;
            }

            if let Some((span_end, pieces)) = split_text_command_regions(text, index, rewrite_headings)
            {
                push_region(&mut regions, TextRegion::editable(&text[last..index]));
                for piece in pieces {
                    push_region(&mut regions, piece);
                }
                index = span_end;
                last = span_end;
                continue;
            }

            if let Some(end) = find_command_span_end(text, index) {
                push_region(&mut regions, TextRegion::editable(&text[last..index]));
                push_region(&mut regions, TextRegion::locked_text(&text[index..end]));
                index = end;
                last = end;
                continue;
            }
        }

        index += step;
    }

    push_region(&mut regions, TextRegion::editable(&text[last..]));
    if regions.is_empty() {
        return vec![TextRegion::editable(text)];
    }
    regions
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

fn push_region(regions: &mut Vec<TextRegion>, region: TextRegion) {
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

fn split_text_command_regions(text: &str, index: usize, rewrite_headings: bool) -> Option<(usize, Vec<TextRegion>)> {
    let (name, mut pos) = parse_command_name(text, index)?;
    let name = name?;

    let is_heading_command = HEADING_COMMANDS.contains(&name);
    let allow_single_arg = TEXT_COMMANDS.contains(&name);
    let is_link_text_command = LINK_TEXT_COMMANDS.contains(&name);
    if !is_heading_command && !allow_single_arg && !is_link_text_command {
        return None;
    }

    let bytes = text.as_bytes();
    loop {
        pos = consume_whitespace(text, pos);
        if pos >= bytes.len() {
            return None;
        }
        if bytes[pos] == b'[' {
            pos = parse_bracket_group(text, pos)?;
            continue;
        }
        break;
    }

    if is_link_text_command {
        return split_link_text_command_regions(text, index, pos, rewrite_headings);
    }

    if bytes.get(pos) != Some(&b'{') {
        return None;
    }

    let group_end = parse_brace_group(text, pos)?;
    if group_end <= pos + 1 {
        return None;
    }
    let content_start = pos + 1;
    let content_end = group_end - 1;

    if is_heading_command && !rewrite_headings {
        return Some((
            group_end,
            vec![
                TextRegion::syntax_token(&text[index..content_start]),
                TextRegion::locked_text(&text[content_start..content_end]),
                TextRegion::syntax_token(&text[content_end..group_end]),
            ],
        ));
    }

    let mut out = vec![TextRegion::syntax_token(&text[index..content_start])];
    out.extend(mark_editable_regions_atomic(parse_regions(
        &text[content_start..content_end],
        rewrite_headings,
    )));
    out.push(TextRegion::syntax_token(&text[content_end..group_end]));

    Some((group_end, out))
}

fn split_link_text_command_regions(
    text: &str,
    index: usize,
    first_group_start: usize,
    rewrite_headings: bool,
) -> Option<(usize, Vec<TextRegion>)> {
    let first_group_end = parse_brace_group(text, first_group_start)?;
    if first_group_end <= first_group_start + 1 {
        return None;
    }

    let second_group_start = consume_whitespace(text, first_group_end);
    if text.as_bytes().get(second_group_start) != Some(&b'{') {
        return None;
    }

    let second_group_end = parse_brace_group(text, second_group_start)?;
    if second_group_end <= second_group_start + 1 {
        return None;
    }

    let url_start = first_group_start + 1;
    let url_end = first_group_end - 1;
    let label_start = second_group_start + 1;
    let label_end = second_group_end - 1;

    let mut out = vec![TextRegion::syntax_token(&text[index..url_start])];
    out.push(TextRegion::inline_object(&text[url_start..url_end]));
    out.push(TextRegion::syntax_token(&text[url_end..label_start]));
    out.extend(mark_editable_regions_atomic(parse_regions(
        &text[label_start..label_end],
        rewrite_headings,
    )));
    out.push(TextRegion::syntax_token(&text[label_end..second_group_end]));
    Some((second_group_end, out))
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

fn parse_command_name(text: &str, index: usize) -> Option<(Option<&str>, usize)> {
    let bytes = text.as_bytes();
    if index >= bytes.len() || bytes[index] != b'\\' {
        return None;
    }

    let mut pos = index + 1;
    if pos >= bytes.len() {
        return None;
    }

    if bytes[pos].is_ascii_alphabetic() {
        let start = pos;
        while pos < bytes.len() && bytes[pos].is_ascii_alphabetic() {
            pos += 1;
        }
        let end = pos;
        if pos < bytes.len() && bytes[pos] == b'*' {
            pos += 1;
        }
        return Some((Some(&text[start..end]), pos));
    }

    pos += 1;
    Some((None, pos))
}
