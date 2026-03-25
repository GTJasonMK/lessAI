#[derive(Debug, Default, Clone, Copy)]
struct ScriptStats {
    cjk: usize,
    latin: usize,
    digits: usize,
    total: usize,
}

fn is_cjk_char(ch: char) -> bool {
    matches!(
        ch,
        '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}'
    )
}

fn script_stats(text: &str) -> ScriptStats {
    let mut stats = ScriptStats::default();

    for ch in text.chars() {
        if ch.is_whitespace() {
            continue;
        }
        if is_cjk_char(ch) {
            stats.cjk = stats.cjk.saturating_add(1);
            stats.total = stats.total.saturating_add(1);
            continue;
        }
        if ch.is_ascii_alphabetic() {
            stats.latin = stats.latin.saturating_add(1);
            stats.total = stats.total.saturating_add(1);
            continue;
        }
        if ch.is_ascii_digit() {
            stats.digits = stats.digits.saturating_add(1);
            stats.total = stats.total.saturating_add(1);
            continue;
        }
    }

    stats
}

fn is_mostly_cjk(stats: ScriptStats) -> bool {
    stats.total >= 20 && stats.cjk.saturating_mul(100) / stats.total >= 40
}

fn is_mostly_latin(stats: ScriptStats) -> bool {
    stats.total >= 20 && stats.latin.saturating_mul(100) / stats.total >= 60
}

fn find_unwanted_meta_pattern(text: &str) -> Option<&'static str> {
    // 英文自我介绍/免责声明模板（最常见的“污染正文”来源）
    const META_PATTERNS_EN: &[&str] = &[
        "i am claude",
        "made by anthropic",
        "helpful, harmless, and honest",
        "i'm an ai assistant",
        "i am an ai assistant",
        "as an ai language model",
        "as an ai assistant",
        "happy to help you",
        "i don't have information about the specific model version",
        "i don't have information about the specific model version or id",
    ];

    let lowered = text.to_ascii_lowercase();
    for pattern in META_PATTERNS_EN.iter() {
        if lowered.contains(pattern) {
            return Some(*pattern);
        }
    }

    // 中文自我介绍/免责声明（保守只抓非常明显的模板，避免误伤正文）
    const META_PATTERNS_ZH: &[&str] = &[
        "我是一个ai助手",
        "我是一名ai助手",
        "作为一个ai助手",
        "作为一名ai助手",
        "作为ai语言模型",
        "作为一个ai语言模型",
        "作为一名ai语言模型",
        "作为一个人工智能助手",
        "我无法访问",
        "我不能访问",
    ];

    for pattern in META_PATTERNS_ZH.iter() {
        if text.contains(pattern) {
            return Some(*pattern);
        }
    }

    None
}

fn is_boundary_after_prefix(rest: &str) -> bool {
    rest.is_empty()
        || rest.chars().next().is_some_and(|ch| {
            ch.is_whitespace()
                || matches!(
                    ch,
                    '，' | ',' | '。' | '.' | '！' | '!' | '？' | '?' | ':' | '：' | ';' | '；'
                )
        })
}

fn starts_with_phrase(text: &str, phrase: &str) -> bool {
    if !text.starts_with(phrase) {
        return false;
    }
    let rest = &text[phrase.len()..];
    is_boundary_after_prefix(rest)
}

fn starts_with_any_phrase(text: &str, phrases: &[&'static str]) -> Option<&'static str> {
    for phrase in phrases.iter() {
        if starts_with_phrase(text, phrase) {
            return Some(*phrase);
        }
    }
    None
}

fn first_nonempty_line(text: &str) -> &str {
    for line in text.lines() {
        if !line.trim().is_empty() {
            return line.trim_start();
        }
    }
    text.trim_start()
}

fn find_unwanted_preface(source: &str, candidate: &str) -> Option<&'static str> {
    let source_line = first_nonempty_line(source.trim_start());
    let candidate_line = first_nonempty_line(candidate.trim_start());

    // 问候语：文档正文通常不会平白无故多出来一句“你好/Hello”。
    // 但如果原文就是书信/对话体开头的问候，允许其存在（甚至允许变体改写）。
    const GREETINGS_ZH: &[&str] = &["你好", "您好", "嗨", "哈喽", "早上好", "下午好", "晚上好"];
    let source_has_greeting = starts_with_any_phrase(source_line, GREETINGS_ZH).is_some();
    if starts_with_any_phrase(candidate_line, GREETINGS_ZH).is_some() && !source_has_greeting {
        return Some("问候语");
    }

    let source_lower = source_line.to_ascii_lowercase();
    let candidate_lower = candidate_line.to_ascii_lowercase();
    const GREETINGS_EN: &[&str] = &["hi", "hello", "hey"];
    let source_has_greeting_en = starts_with_any_phrase(&source_lower, GREETINGS_EN).is_some();
    if starts_with_any_phrase(&candidate_lower, GREETINGS_EN).is_some() && !source_has_greeting_en {
        return Some("greeting");
    }

    // 明显的“助手客套开场”模板：如果原文没有，对正文来说基本都是污染。
    const PREFACE_ZH: &[&str] = &["当然可以", "没问题", "好的", "可以的"];
    let source_has_preface = starts_with_any_phrase(source_line, PREFACE_ZH).is_some();
    if let Some(matched) = starts_with_any_phrase(candidate_line, PREFACE_ZH) {
        if !source_has_preface {
            return Some(matched);
        }
    }

    None
}

pub(super) fn validate_rewrite_output(source: &str, candidate: &str) -> Result<(), String> {
    if candidate.trim().is_empty() {
        return Err("模型输出为空。".to_string());
    }

    if let Some(pattern) = find_unwanted_meta_pattern(candidate) {
        let source_lower = source.to_ascii_lowercase();
        let candidate_lower = candidate.to_ascii_lowercase();
        let in_source = source.contains(pattern) || source_lower.contains(pattern);
        let in_candidate = candidate.contains(pattern) || candidate_lower.contains(pattern);
        if in_candidate && !in_source {
            return Err(format!(
                "模型输出疑似自我介绍/免责声明（命中：{pattern}）。"
            ));
        }
    }

    if let Some(pattern) = find_unwanted_preface(source, candidate) {
        return Err(format!("模型输出疑似客套/问候开场（命中：{pattern}）。"));
    }

    let source_stats = script_stats(source);
    let candidate_stats = script_stats(candidate);

    // 语言脚本明显不一致：大概率是模型跑题或误触发了“默认自我介绍”。
    if (is_mostly_cjk(source_stats) && is_mostly_latin(candidate_stats))
        || (is_mostly_latin(source_stats) && is_mostly_cjk(candidate_stats))
    {
        return Err("模型输出语言与原文不匹配（疑似跑题）。".to_string());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_rewrite_output;

    #[test]
    fn rejects_unwanted_greeting_preface_when_source_has_no_greeting() {
        let source = "第一句话是正文，不是问候。";
        let candidate = "你好！第一句话是正文。";
        assert!(validate_rewrite_output(source, candidate).is_err());
    }

    #[test]
    fn allows_greeting_when_source_itself_starts_with_greeting() {
        let source = "你好，张三：\n这是一封信。";
        let candidate = "您好，张三：\n这是一封信。";
        assert!(validate_rewrite_output(source, candidate).is_ok());
    }
}
