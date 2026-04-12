#[derive(Debug, Clone)]
pub(super) struct MaskedParagraphBlock {
    pub body: Vec<char>,
    pub editable: Vec<bool>,
    pub separator_after: String,
}

pub(super) fn split_masked_paragraph_blocks(
    chars: &[char],
    editable: &[bool],
) -> Vec<MaskedParagraphBlock> {
    let lines = split_masked_lines(chars, editable);
    build_paragraph_blocks(lines)
}

fn split_masked_lines(chars: &[char], editable: &[bool]) -> Vec<(Vec<char>, Vec<bool>)> {
    let mut lines: Vec<(Vec<char>, Vec<bool>)> = Vec::new();
    let mut current_chars = Vec::new();
    let mut current_editable = Vec::new();
    let mut index = 0usize;

    while index < chars.len() {
        current_chars.push(chars[index]);
        current_editable.push(editable.get(index).copied().unwrap_or(false));
        if chars[index] == '\n' {
            lines.push((
                std::mem::take(&mut current_chars),
                std::mem::take(&mut current_editable),
            ));
            index += 1;
            continue;
        }
        if chars[index] == '\r' {
            if index + 1 < chars.len() && chars[index + 1] == '\n' {
                current_chars.push(chars[index + 1]);
                current_editable.push(editable.get(index + 1).copied().unwrap_or(false));
                index += 1;
            }
            lines.push((
                std::mem::take(&mut current_chars),
                std::mem::take(&mut current_editable),
            ));
        }
        index += 1;
    }

    if !current_chars.is_empty() || chars.is_empty() {
        lines.push((current_chars, current_editable));
    }
    lines
}

fn build_paragraph_blocks(lines: Vec<(Vec<char>, Vec<bool>)>) -> Vec<MaskedParagraphBlock> {
    let mut blocks = Vec::new();
    let mut current_body = Vec::new();
    let mut current_editable = Vec::new();
    let mut current_separator = String::new();
    let mut in_separator = false;

    for (line_chars, line_editable) in lines {
        let is_blank = line_chars.iter().all(|ch| ch.is_whitespace());
        let line_text = line_chars.iter().collect::<String>();

        if in_separator && !is_blank {
            blocks.push(MaskedParagraphBlock {
                body: std::mem::take(&mut current_body),
                editable: std::mem::take(&mut current_editable),
                separator_after: std::mem::take(&mut current_separator),
            });
            in_separator = false;
        }

        if is_blank {
            current_separator.push_str(&line_text);
            in_separator = true;
            continue;
        }

        current_body.extend_from_slice(&line_chars);
        current_editable.extend_from_slice(&line_editable);
    }

    blocks.push(MaskedParagraphBlock {
        body: current_body,
        editable: current_editable,
        separator_after: current_separator,
    });
    blocks
}
