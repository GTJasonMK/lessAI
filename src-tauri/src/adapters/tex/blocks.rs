use super::block_support::{
    classify_text_block_kind, collect_blank_lines, find_locked_block_end,
    heading_command_block_end, is_heading_command_line, is_item_line, is_list_environment_begin,
    is_list_environment_end, slice_text, split_lines_with_offsets,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TexBlock {
    pub kind: &'static str,
    pub text: String,
}

pub(super) fn scan_blocks(text: &str) -> Vec<TexBlock> {
    let lines = split_lines_with_offsets(text);
    let mut blocks: Vec<TexBlock> = Vec::new();
    let mut pending_prefix = String::new();
    let mut index = 0usize;

    while index < lines.len() {
        let line = lines[index].line;
        if line.trim().is_empty() {
            let blank = collect_blank_lines(text, &lines, index);
            if let Some(last) = blocks.last_mut() {
                last.text.push_str(&blank.text);
            } else {
                pending_prefix.push_str(&blank.text);
            }
            index = blank.next_index;
            continue;
        }

        if let Some((end, kind)) = find_locked_block_end(&lines, index) {
            let mut block_text = std::mem::take(&mut pending_prefix);
            block_text.push_str(&slice_text(text, &lines, index, end));
            let blank = collect_blank_lines(text, &lines, end);
            block_text.push_str(&blank.text);
            blocks.push(TexBlock {
                kind,
                text: block_text,
            });
            index = blank.next_index;
            continue;
        }

        if is_list_environment_begin(line) {
            pending_prefix.push_str(&slice_text(text, &lines, index, index + 1));
            index += 1;
            continue;
        }

        if is_list_environment_end(line) {
            let end_text = slice_text(text, &lines, index, index + 1);
            if let Some(last) = blocks.last_mut() {
                last.text.push_str(&end_text);
            } else {
                pending_prefix.push_str(&end_text);
            }
            index += 1;
            continue;
        }

        if is_heading_command_line(line) {
            let mut block_text = std::mem::take(&mut pending_prefix);
            let command_end = heading_command_block_end(text, &lines, index).unwrap_or(index + 1);
            block_text.push_str(&slice_text(text, &lines, index, command_end));
            let blank = collect_blank_lines(text, &lines, command_end);
            block_text.push_str(&blank.text);
            blocks.push(TexBlock {
                kind: "command_block",
                text: block_text,
            });
            index = blank.next_index;
            continue;
        }

        if is_item_line(line) {
            let mut block_text = std::mem::take(&mut pending_prefix);
            block_text.push_str(&slice_text(text, &lines, index, index + 1));
            index += 1;
            while index < lines.len() {
                let next = lines[index].line;
                if next.trim().is_empty()
                    || is_item_line(next)
                    || is_heading_command_line(next)
                    || is_list_environment_end(next)
                {
                    break;
                }
                block_text.push_str(&slice_text(text, &lines, index, index + 1));
                index += 1;
            }
            let blank = collect_blank_lines(text, &lines, index);
            block_text.push_str(&blank.text);
            blocks.push(TexBlock {
                kind: classify_text_block_kind(&block_text, Some("command_block")),
                text: block_text,
            });
            index = blank.next_index;
            continue;
        }

        let mut block_text = std::mem::take(&mut pending_prefix);
        block_text.push_str(&slice_text(text, &lines, index, index + 1));
        index += 1;
        while index < lines.len() {
            let next = lines[index].line;
            if next.trim().is_empty()
                || is_heading_command_line(next)
                || is_item_line(next)
                || is_list_environment_begin(next)
                || is_list_environment_end(next)
                || find_locked_block_end(&lines, index).is_some()
            {
                break;
            }
            block_text.push_str(&slice_text(text, &lines, index, index + 1));
            index += 1;
        }
        let blank = collect_blank_lines(text, &lines, index);
        block_text.push_str(&blank.text);
        blocks.push(TexBlock {
            kind: classify_text_block_kind(&block_text, None),
            text: block_text,
        });
        index = blank.next_index;
    }

    if !pending_prefix.is_empty() {
        blocks.push(TexBlock {
            kind: classify_text_block_kind(&pending_prefix, None),
            text: pending_prefix,
        });
    }

    blocks
}
