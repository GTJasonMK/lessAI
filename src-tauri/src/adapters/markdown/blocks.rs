use super::block_support::{
    continues_list_or_quote_block, find_yaml_front_matter_range, is_table_row, is_table_start,
    push_block, push_block_with_trailing_blanks, split_lines_with_offsets,
    starts_standalone_markdown_block, MarkdownBlock,
};
use super::syntax::{
    detect_fence_marker, is_atx_heading_line, is_fence_close, is_html_like_line,
    is_horizontal_rule_line, is_indented_code_line, is_list_or_quote_line,
    is_math_block_delimiter_line, is_reference_definition_line, is_setext_underline_line,
};

pub(super) fn scan_blocks(text: &str) -> Vec<MarkdownBlock> {
    let lines = split_lines_with_offsets(text);
    let front_matter = find_yaml_front_matter_range(&lines);
    let mut blocks = Vec::new();
    let mut index = 0usize;

    while index < lines.len() {
        if let Some((start, end)) = front_matter {
            if index == start {
                index = push_block_with_trailing_blanks(
                    &mut blocks,
                    text,
                    &lines,
                    start,
                    end + 1,
                    "locked_block",
                );
                continue;
            }
        }

        let line = lines[index].line;
        if line.trim().is_empty() {
            index = push_block(&mut blocks, text, &lines, index, index + 1, "blank");
            continue;
        }

        if let Some(marker) = detect_fence_marker(line) {
            let mut end = index + 1;
            while end < lines.len() {
                if is_fence_close(lines[end].line, marker) {
                    end += 1;
                    break;
                }
                end += 1;
            }
            index = push_block_with_trailing_blanks(
                &mut blocks,
                text,
                &lines,
                index,
                end,
                "locked_block",
            );
            continue;
        }

        if is_math_block_delimiter_line(line) {
            let mut end = index + 1;
            while end < lines.len() {
                if is_math_block_delimiter_line(lines[end].line) {
                    end += 1;
                    break;
                }
                end += 1;
            }
            index = push_block_with_trailing_blanks(
                &mut blocks,
                text,
                &lines,
                index,
                end,
                "locked_block",
            );
            continue;
        }

        if is_table_start(&lines, index) {
            let mut end = index + 2;
            while end < lines.len() && is_table_row(lines[end].line) {
                end += 1;
            }
            index = push_block_with_trailing_blanks(
                &mut blocks,
                text,
                &lines,
                index,
                end,
                "locked_block",
            );
            continue;
        }

        if is_atx_heading_line(line) {
            index = push_block_with_trailing_blanks(
                &mut blocks,
                text,
                &lines,
                index,
                index + 1,
                "heading",
            );
            continue;
        }

        if index + 1 < lines.len()
            && !line.trim().is_empty()
            && is_setext_underline_line(lines[index + 1].line)
        {
            index = push_block_with_trailing_blanks(
                &mut blocks,
                text,
                &lines,
                index,
                index + 2,
                "heading",
            );
            continue;
        }

        if is_reference_definition_line(line)
            || is_html_like_line(line)
            || is_horizontal_rule_line(line)
            || is_indented_code_line(line)
        {
            let mut end = index + 1;
            if is_indented_code_line(line) {
                while end < lines.len() && is_indented_code_line(lines[end].line) {
                    end += 1;
                }
            }
            index = push_block_with_trailing_blanks(
                &mut blocks,
                text,
                &lines,
                index,
                end,
                "locked_block",
            );
            continue;
        }

        if is_list_or_quote_line(line) {
            let kind = if line.trim_start().starts_with('>') {
                "quote"
            } else {
                "list_item"
            };
            let mut end = index + 1;
            while end < lines.len() {
                let next = lines[end].line;
                if next.trim().is_empty() {
                    break;
                }
                if continues_list_or_quote_block(kind, line, next) {
                    end += 1;
                    continue;
                }
                if starts_standalone_markdown_block(&lines, end) {
                    break;
                }
                end += 1;
            }
            index = push_block_with_trailing_blanks(&mut blocks, text, &lines, index, end, kind);
            continue;
        }

        let mut end = index + 1;
        while end < lines.len() {
            let next = lines[end].line;
            if next.trim().is_empty() || starts_standalone_markdown_block(&lines, end) {
                break;
            }
            end += 1;
        }
        index = push_block_with_trailing_blanks(
            &mut blocks,
            text,
            &lines,
            index,
            end,
            "paragraph",
        );
    }

    blocks
}
