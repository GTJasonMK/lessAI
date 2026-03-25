pub fn normalize_text(input: &str) -> String {
    let normalized = input.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines = Vec::new();
    let mut blank_streak = 0usize;

    for raw_line in normalized.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            blank_streak += 1;
            if blank_streak <= 1 {
                lines.push(String::new());
            }
        } else {
            blank_streak = 0;
            lines.push(trimmed.to_string());
        }
    }

    lines.join("\n").trim().to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    CrLf,
    Lf,
    Cr,
    None,
}

pub fn detect_line_ending(text: &str) -> LineEnding {
    let bytes = text.as_bytes();
    let mut index = 0usize;
    let mut crlf = 0usize;
    let mut lf = 0usize;
    let mut cr = 0usize;

    while index < bytes.len() {
        match bytes[index] {
            b'\r' => {
                if index + 1 < bytes.len() && bytes[index + 1] == b'\n' {
                    crlf = crlf.saturating_add(1);
                    index += 2;
                } else {
                    cr = cr.saturating_add(1);
                    index += 1;
                }
            }
            b'\n' => {
                lf = lf.saturating_add(1);
                index += 1;
            }
            _ => index += 1,
        }
    }

    if crlf == 0 && lf == 0 && cr == 0 {
        return LineEnding::None;
    }

    if crlf >= lf && crlf >= cr {
        LineEnding::CrLf
    } else if lf >= cr {
        LineEnding::Lf
    } else {
        LineEnding::Cr
    }
}

pub(super) fn normalize_line_endings_to_lf(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

pub fn convert_line_endings(text: &str, ending: LineEnding) -> String {
    let normalized = normalize_line_endings_to_lf(text);
    match ending {
        LineEnding::CrLf => normalized.replace('\n', "\r\n"),
        LineEnding::Cr => normalized.replace('\n', "\r"),
        LineEnding::Lf | LineEnding::None => normalized,
    }
}

pub fn strip_trailing_spaces_per_line(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut index = 0usize;

    while index < bytes.len() {
        let line_start = index;
        while index < bytes.len() && bytes[index] != b'\r' && bytes[index] != b'\n' {
            index += 1;
        }

        let mut line_end = index;
        while line_end > line_start && matches!(bytes[line_end - 1], b' ' | b'\t') {
            line_end -= 1;
        }
        out.push_str(&text[line_start..line_end]);

        if index >= bytes.len() {
            break;
        }

        if bytes[index] == b'\r' {
            if index + 1 < bytes.len() && bytes[index + 1] == b'\n' {
                out.push_str("\r\n");
                index += 2;
            } else {
                out.push('\r');
                index += 1;
            }
        } else {
            out.push('\n');
            index += 1;
        }
    }

    out
}

pub fn collapse_line_breaks_to_spaces(text: &str) -> String {
    if !text.contains('\n') && !text.contains('\r') {
        return text.to_string();
    }

    let normalized = normalize_line_endings_to_lf(text);
    let mut out = String::with_capacity(normalized.len());
    let mut last_was_space = false;

    for ch in normalized.chars() {
        if ch == '\n' {
            if !last_was_space {
                out.push(' ');
                last_was_space = true;
            }
            continue;
        }

        out.push(ch);
        last_was_space = ch == ' ';
    }

    out.trim().to_string()
}

pub(super) fn trim_ascii_spaces_tabs_start(text: &str) -> &str {
    let bytes = text.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() && matches!(bytes[index], b' ' | b'\t') {
        index += 1;
    }
    &text[index..]
}

fn trim_ascii_spaces_tabs_end(text: &str) -> &str {
    let bytes = text.as_bytes();
    let mut end = bytes.len();
    while end > 0 && matches!(bytes[end - 1], b' ' | b'\t') {
        end -= 1;
    }
    &text[..end]
}

fn trim_ascii_spaces_tabs(text: &str) -> &str {
    let trimmed_end = trim_ascii_spaces_tabs_end(text);
    trim_ascii_spaces_tabs_start(trimmed_end)
}

fn detect_line_marker_len(rest: &str) -> usize {
    let mut iter = rest.char_indices();
    let Some((_, first)) = iter.next() else {
        return 0;
    };

    // Markdown 标题：### ...
    if first == '#' {
        let mut end = first.len_utf8();
        for (index, ch) in iter {
            if ch == '#' {
                end = index + ch.len_utf8();
            } else {
                break;
            }
        }
        return end;
    }

    // 引用：>>> ...
    if first == '>' {
        let mut end = first.len_utf8();
        for (index, ch) in iter {
            if ch == '>' {
                end = index + ch.len_utf8();
            } else {
                break;
            }
        }
        return end;
    }

    // 无序列表：- / * / + / • / ·
    if matches!(first, '-' | '*' | '+') || matches!(first, '•' | '·') {
        return first.len_utf8();
    }

    // 有序列表：1. / 1) / 1）/ 1、/ 1．
    if first.is_ascii_digit() {
        let mut digits_end = first.len_utf8();
        for (index, ch) in iter {
            if ch.is_ascii_digit() {
                digits_end = index + ch.len_utf8();
            } else {
                break;
            }
        }

        let after = &rest[digits_end..];
        let Some(marker) = after.chars().next() else {
            return 0;
        };
        if matches!(marker, '.' | '．' | ')' | '）' | '、') {
            return digits_end + marker.len_utf8();
        }
        return 0;
    }

    // 括号编号：（1）/(1)
    if matches!(first, '(' | '（') {
        let closing = if first == '(' { ')' } else { '）' };
        let mut count = 0usize;
        for (index, ch) in rest.char_indices().skip(1) {
            count = count.saturating_add(1);
            if count > 12 {
                break;
            }
            if ch == closing {
                return index + ch.len_utf8();
            }
        }
    }

    0
}

pub(super) fn split_line_skeleton(line: &str) -> (String, String, String) {
    // 说明：
    // - 该函数用于“格式骨架锁定”：尽量把缩进、列表符号、编号、引用前缀等视为不可变格式；
    // - 让模型只改写核心正文，避免空格/缩进/列表结构漂移。
    //
    // 这里仅把行尾空格/制表符视为 suffix；其他 Unicode 空白（例如 NBSP）更可能是正文的一部分。
    let base = trim_ascii_spaces_tabs_end(line);
    let suffix = line[base.len()..].to_string();

    let bytes = base.as_bytes();
    let mut indent_end = 0usize;
    while indent_end < bytes.len() && matches!(bytes[indent_end], b' ' | b'\t') {
        indent_end += 1;
    }

    let rest = &base[indent_end..];
    let marker_len = detect_line_marker_len(rest);
    let mut prefix_end = indent_end.saturating_add(marker_len);

    while prefix_end < bytes.len() && matches!(bytes[prefix_end], b' ' | b'\t') {
        prefix_end += 1;
    }

    let prefix = base[..prefix_end].to_string();
    let core = base[prefix_end..].to_string();

    (prefix, core, suffix)
}

pub(super) fn strip_redundant_prefix(candidate: &str, source_prefix: &str) -> String {
    // candidate 可能会把原有的列表符号/编号再输出一遍，这会导致我们 reattach prefix 时重复。
    // 这里做一次“去重”：如果 candidate（去掉前导空格/制表符）仍以 prefix（去掉缩进）开头，则剥离它。
    let mut body = trim_ascii_spaces_tabs(candidate);

    let marker = trim_ascii_spaces_tabs_start(source_prefix);
    if !marker.is_empty() && body.starts_with(marker) {
        body = &body[marker.len()..];
        body = trim_ascii_spaces_tabs(body);
    }

    body.to_string()
}

pub(super) fn enforce_line_skeleton(source_line: &str, candidate_line: &str) -> String {
    // 空行/仅空白：完全属于格式骨架，原样保留（包括空格缩进）。
    if source_line.trim().is_empty() {
        return source_line.to_string();
    }

    let (prefix, core, suffix) = split_line_skeleton(source_line);

    // 只有前缀（例如 "- "）也属于格式骨架，避免模型“补全”内容造成漂移。
    if core.trim().is_empty() {
        return source_line.to_string();
    }

    let body = strip_redundant_prefix(candidate_line, &prefix);
    if body.trim().is_empty() {
        return source_line.to_string();
    }

    format!("{prefix}{body}{suffix}")
}

pub fn has_trailing_spaces_per_line(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut index = 0usize;
    let mut line_start = 0usize;

    while index < bytes.len() {
        if bytes[index] == b'\r' || bytes[index] == b'\n' {
            if index > line_start && matches!(bytes[index - 1], b' ' | b'\t') {
                return true;
            }

            if bytes[index] == b'\r' && index + 1 < bytes.len() && bytes[index + 1] == b'\n' {
                index += 2;
            } else {
                index += 1;
            }
            line_start = index;
            continue;
        }

        index += 1;
    }

    if bytes.len() > line_start && matches!(bytes[bytes.len() - 1], b' ' | b'\t') {
        return true;
    }

    false
}
