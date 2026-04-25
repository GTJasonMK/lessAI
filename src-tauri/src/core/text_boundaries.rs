const CLAUSE_BOUNDARIES: [char; 10] = ['。', '！', '？', '；', '!', '?', ';', '.', '，', ','];
const CLOSING_PUNCTUATION: [char; 13] = [
    '"', '\'', '”', '’', '）', ')', '】', ']', '}', '」', '』', '》', '〉',
];

pub(crate) fn split_text_chunks_by_paragraph_separator(text: &str) -> Vec<&str> {
    let mut chunks = Vec::new();
    let mut start = 0usize;
    while let Some((_, end)) = find_next_paragraph_separator(text, start) {
        chunks.push(&text[start..end]);
        start = end;
    }
    if start < text.len() || chunks.is_empty() {
        chunks.push(&text[start..]);
    }
    chunks
}

pub(crate) fn contains_paragraph_separator(text: &str) -> bool {
    find_next_paragraph_separator(text, 0).is_some()
}

pub(crate) fn trailing_paragraph_separator_range(text: &str) -> Option<(usize, usize)> {
    let (start, end) = find_next_paragraph_separator(text, 0)?;
    (end == text.len()).then_some((start, end))
}

pub(crate) fn split_text_and_trailing_separator(text: &str) -> (String, String) {
    if let Some((start, end)) = trailing_paragraph_separator_range(text) {
        return (text[..start].to_string(), text[start..end].to_string());
    }
    split_trailing_whitespace(text)
}

pub(crate) fn split_text_chunks_for_rewrite_slots(text: &str) -> Vec<&str> {
    let mut chunks = Vec::new();
    for paragraph_chunk in split_text_chunks_by_paragraph_separator(text) {
        chunks.extend(split_paragraph_chunk_by_clause_boundary(paragraph_chunk));
    }
    chunks
}

fn split_paragraph_chunk_by_clause_boundary(text: &str) -> Vec<&str> {
    let content_limit = trailing_paragraph_separator_range(text)
        .map(|(start, _)| start)
        .unwrap_or(text.len());
    let mut chunks = Vec::new();
    let mut start = 0usize;

    for (index, ch) in text[..content_limit].char_indices() {
        if !is_clause_split_boundary(text, index, ch, content_limit) {
            continue;
        }
        let mut end = index + ch.len_utf8();
        end = consume_closing_punctuation(text, end, content_limit);
        end = consume_inline_whitespace(text, end, content_limit);
        chunks.push(&text[start..end]);
        start = end;
    }

    if start < text.len() || chunks.is_empty() {
        chunks.push(&text[start..]);
    }
    chunks
}

fn is_clause_split_boundary(text: &str, index: usize, ch: char, limit: usize) -> bool {
    if !CLAUSE_BOUNDARIES.contains(&ch) {
        return false;
    }
    if matches!(ch, '.' | ',') {
        let previous = previous_char(text, index);
        let next = next_non_whitespace_char(text, index + ch.len_utf8(), limit);
        if previous.is_some_and(|value| value.is_ascii_digit())
            && next.is_some_and(|value| value.is_ascii_digit())
        {
            return false;
        }
        if ch == '.' {
            if next_char(text, index + ch.len_utf8(), limit) == Some('\\') {
                return false;
            }
            if previous.is_some_and(|value| value.is_ascii_alphabetic())
                && next.is_some_and(|value| value.is_ascii_alphabetic())
            {
                return false;
            }
            if previous.is_some_and(|value| value.is_ascii_alphabetic())
                && next.is_some_and(|value| value.is_ascii_uppercase())
                && ascii_word_len_before(text, index) <= 2
            {
                return false;
            }
        }
    }
    true
}

fn previous_char(text: &str, index: usize) -> Option<char> {
    text[..index].chars().next_back()
}

fn next_char(text: &str, index: usize, limit: usize) -> Option<char> {
    text[index.min(limit)..limit].chars().next()
}

fn next_non_whitespace_char(text: &str, index: usize, limit: usize) -> Option<char> {
    text[index.min(limit)..limit]
        .chars()
        .find(|ch| !ch.is_whitespace())
}

fn ascii_word_len_before(text: &str, index: usize) -> usize {
    text[..index]
        .chars()
        .rev()
        .take_while(|ch| ch.is_ascii_alphabetic())
        .count()
}

fn consume_closing_punctuation(text: &str, mut index: usize, limit: usize) -> usize {
    while let Some(ch) = next_char(text, index, limit) {
        if !CLOSING_PUNCTUATION.contains(&ch) {
            break;
        }
        index += ch.len_utf8();
    }
    index
}

fn consume_inline_whitespace(text: &str, mut index: usize, limit: usize) -> usize {
    while let Some(ch) = next_char(text, index, limit) {
        if !ch.is_whitespace() {
            break;
        }
        index += ch.len_utf8();
    }
    index
}

fn find_next_paragraph_separator(text: &str, from: usize) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut index = from;
    while index < bytes.len() {
        let Some(first_end) = consume_line_break(text, index) else {
            index += 1;
            continue;
        };
        let mut probe = first_end;
        while probe < bytes.len() && matches!(bytes[probe], b' ' | b'\t') {
            probe += 1;
        }
        let Some(mut end) = consume_line_break(text, probe) else {
            index = first_end;
            continue;
        };
        loop {
            let mut next = end;
            while next < bytes.len() && matches!(bytes[next], b' ' | b'\t') {
                next += 1;
            }
            let Some(next_end) = consume_line_break(text, next) else {
                break;
            };
            end = next_end;
        }
        return Some((index, end));
    }
    None
}

fn consume_line_break(text: &str, index: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    match bytes.get(index) {
        Some(b'\n') => Some(index + 1),
        Some(b'\r') if bytes.get(index + 1) == Some(&b'\n') => Some(index + 2),
        Some(b'\r') => Some(index + 1),
        _ => None,
    }
}

fn split_trailing_whitespace(text: &str) -> (String, String) {
    let split_at = text
        .char_indices()
        .rev()
        .find_map(|(index, ch)| (!ch.is_whitespace()).then_some(index + ch.len_utf8()))
        .unwrap_or(0);
    (text[..split_at].to_string(), text[split_at..].to_string())
}
