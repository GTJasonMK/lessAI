#[derive(Debug, Clone, Copy)]
pub(super) enum BoundaryKind {
    Sentence,
    Clause,
}

pub(super) fn is_sentence_boundary(chars: &[char], index: usize) -> bool {
    let ch = chars[index];
    match ch {
        '。' | '！' | '？' | '!' | '?' | '；' | ';' => true,
        '.' => !is_numeric_punctuation(chars, index),
        _ => false,
    }
}

pub(super) fn is_clause_boundary(chars: &[char], index: usize) -> bool {
    let ch = chars[index];
    if is_sentence_boundary(chars, index) {
        return true;
    }

    match ch {
        '，' | '、' | '；' | ';' | '：' | ':' => true,
        ',' => !is_numeric_punctuation(chars, index),
        _ => false,
    }
}

fn is_numeric_punctuation(chars: &[char], index: usize) -> bool {
    let ch = chars[index];
    if !matches!(ch, '.' | ',') {
        return false;
    }

    let prev_is_digit = index
        .checked_sub(1)
        .and_then(|prev| chars.get(prev))
        .map(|value| value.is_ascii_digit())
        .unwrap_or(false);
    let next_is_digit = chars
        .get(index + 1)
        .map(|value| value.is_ascii_digit())
        .unwrap_or(false);
    prev_is_digit && next_is_digit
}

pub(super) fn is_closing_punctuation(ch: char) -> bool {
    matches!(
        ch,
        '"' | '\'' | '”' | '’' | '）' | ')' | '】' | ']' | '}' | '」' | '』' | '》' | '〉'
    )
}
