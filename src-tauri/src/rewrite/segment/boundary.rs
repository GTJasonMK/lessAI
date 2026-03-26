#[derive(Debug, Clone, Copy)]
pub(super) enum BoundaryKind {
    Sentence,
    Clause,
}

fn is_punct_quoted_as_literal(chars: &[char], index: usize) -> bool {
    // 目标：避免把“标点符号本身”当成断句边界。
    //
    // 典型场景：
    // - `整句切分是否在“？”处生效？` 中的 `“？”` 只是“提到问号这个符号”，不是句末问号；
    // - `句号“。”问号“？”逗号“，”` 等测试文本会大量出现这种写法。
    //
    // 我们只处理“紧邻的成对引号/括号”：
    // - opening + [punct] + closing
    // 且 closing 之后还有内容（说明该符号被当作“字面量”嵌在句子里）。
    if index == 0 || index + 1 >= chars.len() {
        return false;
    }
    if index + 2 >= chars.len() {
        // 引号/括号后面没有内容时更像句末（例如 `他说：“？”`），不要强行当字面量。
        return false;
    }

    let prev = chars[index.saturating_sub(1)];
    let next = chars[index + 1];

    // 中文引号
    if (prev == '“' && next == '”') || (prev == '‘' && next == '’') {
        return true;
    }
    // 书名号/角括号
    if (prev == '《' && next == '》') || (prev == '〈' && next == '〉') {
        return true;
    }
    // 日文引号
    if (prev == '「' && next == '」') || (prev == '『' && next == '』') {
        return true;
    }
    // 括号/方括号/书名括号
    if (prev == '(' && next == ')')
        || (prev == '（' && next == '）')
        || (prev == '[' && next == ']')
        || (prev == '【' && next == '】')
    {
        return true;
    }
    // ASCII 引号（同一个字符作为开合）
    if (prev == '"' && next == '"') || (prev == '\'' && next == '\'') {
        return true;
    }

    false
}

fn is_ascii_token_char(ch: char) -> bool {
    // 允许出现在“不可拆分 token”里的 ASCII 字符：
    // - 缩写：U.S.A.
    // - 版本号：v1.2.3
    // - 文件名：report.final.v2.pdf
    // - 邮箱：foo.bar+tag@example.com
    // - URL：https://example.com/a?b=c
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '+' | '#')
}

fn is_period_in_ascii_token(chars: &[char], index: usize) -> bool {
    // 目标：避免把英文缩写/文件名/域名等 token 内部的 `.` 当成句末分割点。
    //
    // 典型场景：
    // - `e.g.` / `i.e.` / `U.S.A.` / `Ph.D.`
    // - `report.final.v2.pdf` / `example.com`
    //
    // 基本原则：
    // - `.` 两侧都是 token 字符（字母/数字等）→ token 内部点，不是句末；
    // - 多 dot 缩写的“最后一个 dot”（A.B. / e.g. / U.S.A.）也不应触发句末切分。
    if chars.get(index) != Some(&'.') {
        return false;
    }

    // report.final / v2.pdf / U.S：dot 夹在 token 字符之间
    let prev_is_token = index
        .checked_sub(1)
        .and_then(|prev| chars.get(prev))
        .map(|value| is_ascii_token_char(*value))
        .unwrap_or(false);
    let next_is_token = chars
        .get(index + 1)
        .map(|value| is_ascii_token_char(*value))
        .unwrap_or(false);
    if prev_is_token && next_is_token {
        return true;
    }

    // e.g. / U.S.A.：最后一个 dot（判断前面是否出现过 `.<token>` 结构）
    let prev_dot_token = index
        .checked_sub(2)
        .and_then(|prev_dot| chars.get(prev_dot).copied())
        == Some('.')
        && index
            .checked_sub(1)
            .and_then(|prev_token| chars.get(prev_token))
            .map(|value| is_ascii_token_char(*value))
            .unwrap_or(false);
    let prev_prev_is_token = index
        .checked_sub(3)
        .and_then(|prev_prev| chars.get(prev_prev))
        .map(|value| is_ascii_token_char(*value))
        .unwrap_or(false);
    if prev_dot_token && prev_prev_is_token {
        return true;
    }

    false
}

fn is_period_in_ellipsis(chars: &[char], index: usize) -> bool {
    // `...` 在英文里经常作为省略号/停顿，不一定代表句末。
    // 为避免被切碎，这里把“连续点号”视为非句末边界。
    chars.get(index) == Some(&'.')
        && (index
            .checked_sub(1)
            .and_then(|prev| chars.get(prev))
            .copied()
            == Some('.')
            || chars.get(index + 1).copied() == Some('.'))
}

fn is_period_after_numeric_list_marker(chars: &[char], index: usize) -> bool {
    // 目标：避免把 “1. 第一条” 这种编号列表的 `.` 当成句末。
    //
    // 典型场景：
    // - `1. 第一条` / `2. 第二条`
    // - 导入 PDF/Word 文本时，编号很常见；若把 `1.` 切成独立 chunk，会导致审阅体验破碎。
    //
    // 规则（保守）：
    // - `.` 前紧邻 1~3 位数字
    // - 数字前是行首/空白/常见前缀括号
    // - `.` 后紧跟空格或 Tab，且后面还有同一行的正文内容
    if chars.get(index) != Some(&'.') {
        return false;
    }

    let mut start = index;
    while start > 0 && chars[start - 1].is_ascii_digit() {
        start = start.saturating_sub(1);
    }
    let digit_len = index.saturating_sub(start);
    if digit_len == 0 || digit_len > 3 {
        return false;
    }

    let prefix_ok = if start == 0 {
        true
    } else {
        let prev = chars[start - 1];
        prev.is_whitespace() || matches!(prev, '(' | '（' | '[' | '【')
    };
    if !prefix_ok {
        return false;
    }

    let mut next = index + 1;
    if next >= chars.len() || !matches!(chars[next], ' ' | '\t') {
        return false;
    }
    while next < chars.len() && matches!(chars[next], ' ' | '\t') {
        next = next.saturating_add(1);
    }
    if next >= chars.len() {
        return false;
    }
    if matches!(chars[next], '\n' | '\r') {
        return false;
    }

    true
}

fn is_colon_in_url_scheme(chars: &[char], index: usize) -> bool {
    // `https://` / `http://` / `file://` 等：`:` 不应触发小句切分。
    chars.get(index) == Some(&':')
        && chars.get(index + 1) == Some(&'/')
        && chars.get(index + 2) == Some(&'/')
}

fn is_fullwidth_colon_in_url_scheme(chars: &[char], index: usize) -> bool {
    // 容错：`http：//`（全角冒号）在复制/输入法里偶尔出现。
    chars.get(index) == Some(&'：')
        && chars.get(index + 1) == Some(&'/')
        && chars.get(index + 2) == Some(&'/')
}

fn is_colon_in_windows_drive(chars: &[char], index: usize) -> bool {
    // `E:\Code` / `C:/Windows`：`:` 不应触发小句切分。
    if chars.get(index) != Some(&':') {
        return false;
    }

    let prev_is_letter = index
        .checked_sub(1)
        .and_then(|prev| chars.get(prev))
        .map(|value| value.is_ascii_alphabetic())
        .unwrap_or(false);
    if !prev_is_letter {
        return false;
    }

    matches!(chars.get(index + 1), Some('\\') | Some('/'))
}

fn is_fullwidth_colon_in_windows_drive(chars: &[char], index: usize) -> bool {
    // 容错：`E：\Code`（全角冒号）
    if chars.get(index) != Some(&'：') {
        return false;
    }

    let prev_is_letter = index
        .checked_sub(1)
        .and_then(|prev| chars.get(prev))
        .map(|value| value.is_ascii_alphabetic())
        .unwrap_or(false);
    if !prev_is_letter {
        return false;
    }

    matches!(chars.get(index + 1), Some('\\') | Some('/'))
}

fn is_colon_in_time_like(chars: &[char], index: usize) -> bool {
    // `12:30` / `1:2`：`:` 不应触发小句切分。
    if chars.get(index) != Some(&':') {
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

fn is_fullwidth_colon_in_time_like(chars: &[char], index: usize) -> bool {
    // `12：30` / `1：2`：`：`（全角冒号）不应触发小句切分。
    if chars.get(index) != Some(&'：') {
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

fn is_inside_url_token(chars: &[char], index: usize) -> bool {
    // 粗略判断某个字符是否位于 URL token 内（避免在 URL 的 `?` 等处断句）。
    // 规则：从当前位置向两侧扩展到空白边界，token 内包含 `://` 则认为是 URL。
    let mut start = index;
    while start > 0 && !chars[start - 1].is_whitespace() {
        start = start.saturating_sub(1);
    }

    let mut end = index;
    while end < chars.len() && !chars[end].is_whitespace() {
        end = end.saturating_add(1);
    }

    let slice = &chars[start..end];
    slice.windows(3).any(|window| window == [':', '/', '/'])
}

pub(super) fn is_sentence_boundary(chars: &[char], index: usize) -> bool {
    let ch = chars[index];
    match ch {
        '。' | '！' | '？' | '；' => !is_punct_quoted_as_literal(chars, index),
        '!' => !is_punct_quoted_as_literal(chars, index) && !is_inside_url_token(chars, index),
        ';' => !is_punct_quoted_as_literal(chars, index) && !is_inside_url_token(chars, index),
        '?' => !is_punct_quoted_as_literal(chars, index) && !is_inside_url_token(chars, index),
        '.' => {
            !is_numeric_punctuation(chars, index)
                && !is_punct_quoted_as_literal(chars, index)
                && !is_period_in_ellipsis(chars, index)
                && !is_period_after_numeric_list_marker(chars, index)
                && !is_period_in_ascii_token(chars, index)
        }
        _ => false,
    }
}

pub(super) fn is_clause_boundary(chars: &[char], index: usize) -> bool {
    let ch = chars[index];
    if is_sentence_boundary(chars, index) {
        return true;
    }

    match ch {
        '，' => !is_punct_quoted_as_literal(chars, index) && !is_numeric_punctuation(chars, index),
        '、' | '；' => !is_punct_quoted_as_literal(chars, index),
        ';' => !is_punct_quoted_as_literal(chars, index) && !is_inside_url_token(chars, index),
        '：' => {
            !is_punct_quoted_as_literal(chars, index)
                && !is_fullwidth_colon_in_url_scheme(chars, index)
                && !is_fullwidth_colon_in_windows_drive(chars, index)
                && !is_fullwidth_colon_in_time_like(chars, index)
        }
        ':' => {
            !is_punct_quoted_as_literal(chars, index)
                && !is_colon_in_url_scheme(chars, index)
                && !is_inside_url_token(chars, index)
                && !is_colon_in_windows_drive(chars, index)
                && !is_colon_in_time_like(chars, index)
        }
        ',' => !is_numeric_punctuation(chars, index) && !is_punct_quoted_as_literal(chars, index),
        _ => false,
    }
}

fn is_numeric_punctuation(chars: &[char], index: usize) -> bool {
    let ch = chars[index];
    // 数字里常见的分隔符：小数点/千分位分隔符。
    // 兼容中文输入法可能产生的全角逗号 `，`。
    if !matches!(ch, '.' | ',' | '，') {
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
