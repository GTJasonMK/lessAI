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

fn normalize_line_for_preface_detection(line: &str) -> String {
    // 说明：
    // - rewrite/plain 会对每一行做“格式骨架锁定”，例如保留 `- ` / `1. ` / `> ` / `### ` 等前缀；
    // - 模型偶尔会在这些前缀之后插入“你好/当然可以”等客套开场；
    // - 如果只检查“整行行首”，会漏掉这种污染。
    //
    // 这里用 split_line_skeleton 抽出 core（去掉常见结构前缀后剩余的正文），
    // 并优先对 core 做问候/客套检测。
    let trimmed = line.trim_start();
    let (prefix, core, _) = super::super::text::split_line_skeleton(trimmed);

    // 只有在 prefix 本身是“结构化前缀”（通常以空格/制表符收尾）时才使用 core。
    // 例如：`- 你好` / `1. 你好` / `> 你好` / `### 你好`。
    let prefix_looks_structural = prefix
        .chars()
        .last()
        .is_some_and(|ch| ch == ' ' || ch == '\t');

    if prefix_looks_structural && !core.trim().is_empty() {
        return core.trim_start().to_string();
    }

    trimmed.to_string()
}

fn find_unwanted_preface(source: &str, candidate: &str) -> Option<&'static str> {
    let source_line_raw = first_nonempty_line(source.trim_start());
    let candidate_line_raw = first_nonempty_line(candidate.trim_start());
    let source_line = normalize_line_for_preface_detection(source_line_raw);
    let candidate_line = normalize_line_for_preface_detection(candidate_line_raw);

    // 问候语：文档正文通常不会平白无故多出来一句“你好/Hello”。
    // 但如果原文就是书信/对话体开头的问候，允许其存在（甚至允许变体改写）。
    const GREETINGS_ZH: &[&str] = &["你好", "您好", "嗨", "哈喽", "早上好", "下午好", "晚上好"];
    let source_has_greeting = starts_with_any_phrase(&source_line, GREETINGS_ZH).is_some();
    if starts_with_any_phrase(&candidate_line, GREETINGS_ZH).is_some() && !source_has_greeting {
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
    let source_has_preface = starts_with_any_phrase(&source_line, PREFACE_ZH).is_some();
    if let Some(matched) = starts_with_any_phrase(&candidate_line, PREFACE_ZH) {
        if !source_has_preface {
            return Some(matched);
        }
    }

    // 常见的“改写结果引导语”：很多模型会输出
    // - “下面是改写后的版本：...”
    // - “以下是润色后的文本：...”
    // 这种内容对正文来说属于污染，应拒绝并触发重试。
    //
    // 为避免误伤正常正文，这里只在候选首行同时满足：
    // - 以固定引导短语开头
    // - 且含有明显的“改写/润色/降重/优化”元信息关键词
    // 时才判定为不合格。
    const PREFACE_REWRITE_PREFIX: &[&str] = &["下面是", "以下是", "这里是"];
    let meta_hint = candidate_line.contains("改写")
        || candidate_line.contains("润色")
        || candidate_line.contains("降重")
        || candidate_line.contains("优化");
    if meta_hint
        && starts_with_any_phrase(&source_line, PREFACE_REWRITE_PREFIX).is_none()
        && starts_with_any_phrase(&candidate_line, PREFACE_REWRITE_PREFIX).is_some()
    {
        return Some("改写引导语");
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

    #[test]
    fn rejects_rewrite_preface_when_source_has_no_preface() {
        let source = "第一句话是正文，不是提示语。";
        let candidate = "下面是改写后的版本：\n第一句话是正文。";
        assert!(validate_rewrite_output(source, candidate).is_err());
    }

    #[test]
    fn rejects_greeting_inserted_after_list_prefix() {
        let source = "- 第一行是正文。\n第二行继续。";
        let candidate = "- 你好！第一行是正文。\n第二行继续。";
        assert!(validate_rewrite_output(source, candidate).is_err());
    }

    #[test]
    fn rejects_preface_inserted_after_ordered_list_prefix() {
        let source = "1. 第一行是正文。";
        let candidate = "1. 当然可以，第一行是正文。";
        assert!(validate_rewrite_output(source, candidate).is_err());
    }
}
