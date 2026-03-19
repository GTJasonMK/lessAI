use std::time::Duration;

use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::{json, Value};
use std::error::Error as _;

use crate::models::{
    AppSettings, ChunkPreset, DiffSpan, DiffType, ProviderCheckResult,
};

const SYSTEM_PROMPT_FALLBACK: &str = "你是一名严谨的中文文本编辑。你的任务是对给定片段进行自然化改写，让表达更像真实人工写作，但必须保持原意、事实、语气和段落层次稳定。不要扩写，不要总结，不要解释，不要输出标题，只输出改写后的正文。";
const SYSTEM_PROMPT_AIGC_V1: &str = include_str!("../../prompt/1.txt");
const SYSTEM_PROMPT_HUMANIZER_ZH: &str = include_str!("../../prompt/2.txt");

fn resolve_system_prompt(settings: &AppSettings) -> String {
    let preset_id = settings.prompt_preset_id.trim();

    let base = match preset_id {
        "aigc_v1" => SYSTEM_PROMPT_AIGC_V1.trim(),
        "humanizer_zh" => SYSTEM_PROMPT_HUMANIZER_ZH.trim(),
        _ => settings
            .custom_prompts
            .iter()
            .find(|item| item.id == preset_id)
            .map(|item| item.content.trim())
            .unwrap_or(""),
    };
    let base = if base.is_empty() { SYSTEM_PROMPT_FALLBACK } else { base };

    let extra = if preset_id == "aigc_v1" {
        "补充约束：最终输出不要包含“修改后/原文”等标签，只输出改写后的正文。"
    } else {
        "补充约束：最终输出只输出改写后的正文，不要输出标题、列表或解释。"
    };

    format!("{base}\n\n{extra}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentedChunk {
    pub text: String,
    /// 该片段后需要拼回去的分隔符（例如空格、换行、段落空行）。
    ///
    /// 设计动机：
    /// - 切块是给 agent/LLM 用的“隐式结构”，不应破坏原文格式；
    /// - 片段之间的空格/换行如果丢失，会导致导出/写回时格式漂移。
    pub separator_after: String,
}

#[derive(Debug, Clone)]
struct ParagraphBlock {
    body: String,
    separator_after: String,
}

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

pub fn normalize_line_endings_to_lf(text: &str) -> String {
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
        while line_end > line_start
            && matches!(bytes[line_end - 1], b' ' | b'\t')
        {
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

fn trim_ascii_spaces_tabs_start(text: &str) -> &str {
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

fn split_line_skeleton(line: &str) -> (String, String, String) {
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

fn strip_redundant_prefix(candidate: &str, source_prefix: &str) -> String {
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

fn enforce_line_skeleton(source_line: &str, candidate_line: &str) -> String {
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

pub fn segment_text(text: &str, preset: ChunkPreset) -> Vec<SegmentedChunk> {
    // 切块目标：
    // - 给 agent/LLM 一个稳定的“工作单元”
    // - 同时保证：把 chunks 拼回去后，原文格式不发生变化（空格/换行/空行都保留）
    //
    // 粒度：
    // - Clause：一小句（逗号/分号等）
    // - Sentence：一整句（句号/问号/感叹号等）
    // - Paragraph：一段话（空行分段）
    //
    // 同时加上硬上限，避免极端长句/长段导致单次调用过重（超限会逐级降级切分）。
    const MAX_CLAUSE_CHARS: usize = 420;
    const MAX_SENTENCE_CHARS: usize = 900;
    const MAX_PARAGRAPH_CHARS: usize = 1_600;

    let blocks = split_paragraph_blocks(text);

    let mut chunks = Vec::new();

    for block in blocks.into_iter() {
        let (body, trailing_ws) = split_trailing_whitespace(&block.body);
        let mut paragraph_separator = trailing_ws;
        paragraph_separator.push_str(&block.separator_after);

        if body.is_empty() {
            append_separator_to_last(&mut chunks, paragraph_separator);
            continue;
        }

        match preset {
            ChunkPreset::Paragraph => {
                if body.chars().count() <= MAX_PARAGRAPH_CHARS {
                    chunks.push(SegmentedChunk {
                        text: body,
                        separator_after: paragraph_separator,
                    });
                    continue;
                }

                let mut pieces =
                    segment_by_sentence(&body, MAX_SENTENCE_CHARS, MAX_CLAUSE_CHARS);
                append_separator_to_last(&mut pieces, paragraph_separator);
                chunks.extend(pieces);
            }
            ChunkPreset::Sentence => {
                let mut pieces =
                    segment_by_sentence(&body, MAX_SENTENCE_CHARS, MAX_CLAUSE_CHARS);
                append_separator_to_last(&mut pieces, paragraph_separator);
                chunks.extend(pieces);
            }
            ChunkPreset::Clause => {
                let mut pieces = segment_by_boundary(&body, BoundaryKind::Clause, MAX_CLAUSE_CHARS);
                append_separator_to_last(&mut pieces, paragraph_separator);
                chunks.extend(pieces);
            }
        }
    }

    if chunks.is_empty() {
        vec![SegmentedChunk {
            text: text.to_string(),
            separator_after: String::new(),
        }]
    } else {
        chunks
    }
}

fn split_paragraph_blocks(text: &str) -> Vec<ParagraphBlock> {
    let bytes = text.as_bytes();
    let mut lines: Vec<(String, String)> = Vec::new();
    let mut start = 0usize;
    let mut index = 0usize;

    while index < bytes.len() {
        match bytes[index] {
            b'\n' => {
                let content = &text[start..index];
                lines.push((content.to_string(), "\n".to_string()));
                index += 1;
                start = index;
            }
            b'\r' => {
                if index + 1 < bytes.len() && bytes[index + 1] == b'\n' {
                    let content = &text[start..index];
                    lines.push((content.to_string(), "\r\n".to_string()));
                    index += 2;
                    start = index;
                } else {
                    let content = &text[start..index];
                    lines.push((content.to_string(), "\r".to_string()));
                    index += 1;
                    start = index;
                }
            }
            _ => index += 1,
        }
    }

    if start < bytes.len() {
        lines.push((text[start..].to_string(), String::new()));
    } else if text.is_empty() {
        lines.push((String::new(), String::new()));
    }

    let mut blocks = Vec::new();
    let mut current_body = String::new();
    let mut current_sep = String::new();
    let mut in_sep = false;

    for (content, ending) in lines.into_iter() {
        let line = format!("{content}{ending}");
        let is_blank = content.trim().is_empty();

        if in_sep {
            if is_blank {
                current_sep.push_str(&line);
            } else {
                blocks.push(ParagraphBlock {
                    body: current_body,
                    separator_after: current_sep,
                });
                current_body = line;
                current_sep = String::new();
                in_sep = false;
            }
            continue;
        }

        if is_blank && !current_body.is_empty() {
            current_sep.push_str(&line);
            in_sep = true;
        } else {
            current_body.push_str(&line);
        }
    }

    blocks.push(ParagraphBlock {
        body: current_body,
        separator_after: current_sep,
    });

    blocks
}

fn split_trailing_whitespace(text: &str) -> (String, String) {
    let trimmed = text.trim_end_matches(|ch: char| ch.is_whitespace());
    let suffix = text[trimmed.len()..].to_string();
    (trimmed.to_string(), suffix)
}

fn append_separator_to_last(chunks: &mut Vec<SegmentedChunk>, separator: String) {
    if separator.is_empty() {
        return;
    }

    if let Some(last) = chunks.last_mut() {
        last.separator_after.push_str(&separator);
    } else {
        chunks.push(SegmentedChunk {
            text: String::new(),
            separator_after: separator,
        });
    }
}

fn segment_by_sentence(
    text: &str,
    max_sentence_chars: usize,
    max_clause_chars: usize,
) -> Vec<SegmentedChunk> {
    let sentences = segment_by_boundary(text, BoundaryKind::Sentence, 0);
    let mut chunks = Vec::new();

    for sentence in sentences.into_iter() {
        if max_sentence_chars == 0 || sentence.text.chars().count() <= max_sentence_chars {
            chunks.push(sentence);
            continue;
        }

        let mut pieces = segment_by_boundary(&sentence.text, BoundaryKind::Clause, max_sentence_chars);
        append_separator_to_last(&mut pieces, sentence.separator_after);

        // Clause 依然可能出现极端长段（没有任何标点），这里再加一个硬上限兜底。
        if max_clause_chars > 0 {
            let mut bounded = Vec::new();
            for piece in pieces.into_iter() {
                if piece.text.chars().count() <= max_sentence_chars {
                    bounded.push(piece);
                    continue;
                }
                let mut hard = segment_by_boundary(&piece.text, BoundaryKind::Clause, max_sentence_chars);
                append_separator_to_last(&mut hard, piece.separator_after);
                bounded.extend(hard);
            }
            chunks.extend(bounded);
        } else {
            chunks.extend(pieces);
        }
    }

    chunks
}

#[derive(Debug, Clone, Copy)]
enum BoundaryKind {
    Sentence,
    Clause,
}

fn segment_by_boundary(text: &str, kind: BoundaryKind, max_chars: usize) -> Vec<SegmentedChunk> {
    let chars: Vec<char> = text.chars().collect();
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut index = 0usize;
    let mut current_len = 0usize;

    while index < chars.len() {
        let ch = chars[index];
        current.push(ch);
        current_len = current_len.saturating_add(1);

        // ⚠️ 格式保真：换行符是“硬边界”
        //
        // 设计意图：
        // - 换行/空行属于文档格式；如果让 LLM 看到并重写这些换行，很容易出现：
        //   - 行数变化（合并/拆分行）
        //   - 列表/标题结构漂移
        //   - 多余空行
        // 因此无论 preset 如何，都强制在换行处分块，并把换行留在 separator_after 里拼回去。
        let is_line_break = matches!(ch, '\n' | '\r');

        let mut should_cut = match kind {
            BoundaryKind::Sentence => is_sentence_boundary(&chars, index),
            BoundaryKind::Clause => is_clause_boundary(&chars, index),
        };
        should_cut = should_cut || is_line_break;

        if should_cut && !is_line_break {
            while index + 1 < chars.len() && is_closing_punctuation(chars[index + 1]) {
                index += 1;
                current.push(chars[index]);
                current_len = current_len.saturating_add(1);
            }
        }

        let hit_max = max_chars > 0 && current_len >= max_chars;

        if should_cut || hit_max {
            let mut separator_after = String::new();
            let mut next = index + 1;
            while next < chars.len() && chars[next].is_whitespace() {
                separator_after.push(chars[next]);
                next += 1;
            }

            let (body, trailing_ws) = split_trailing_whitespace(&current);
            let mut merged_separator = trailing_ws;
            merged_separator.push_str(&separator_after);

            if body.is_empty() {
                append_separator_to_last(&mut chunks, merged_separator);
            } else {
                chunks.push(SegmentedChunk {
                    text: body,
                    separator_after: merged_separator,
                });
            }

            current.clear();
            current_len = 0;
            index = next;
            continue;
        }

        index += 1;
    }

    if !current.is_empty() {
        let (body, trailing_ws) = split_trailing_whitespace(&current);
        if body.is_empty() {
            append_separator_to_last(&mut chunks, trailing_ws);
        } else {
            chunks.push(SegmentedChunk {
                text: body,
                separator_after: trailing_ws,
            });
        }
    }

    chunks
}

fn is_sentence_boundary(chars: &[char], index: usize) -> bool {
    let ch = chars[index];
    match ch {
        '。' | '！' | '？' | '!' | '?' | '；' | ';' => true,
        '.' => !is_numeric_punctuation(chars, index),
        _ => false,
    }
}

fn is_clause_boundary(chars: &[char], index: usize) -> bool {
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

fn is_closing_punctuation(ch: char) -> bool {
    matches!(
        ch,
        '"' | '\''
            | '”'
            | '’'
            | '）'
            | ')'
            | '】'
            | ']'
            | '}'
            | '」'
            | '』'
            | '》'
            | '〉'
    )
}

pub fn build_diff(source: &str, candidate: &str) -> Vec<DiffSpan> {
    let source_chars: Vec<char> = source.chars().collect();
    let candidate_chars: Vec<char> = candidate.chars().collect();
    let m = source_chars.len();
    let n = candidate_chars.len();
    let mut dp = vec![vec![0usize; n + 1]; m + 1];

    for i in (0..m).rev() {
        for j in (0..n).rev() {
            if source_chars[i] == candidate_chars[j] {
                dp[i][j] = dp[i + 1][j + 1] + 1;
            } else {
                dp[i][j] = dp[i + 1][j].max(dp[i][j + 1]);
            }
        }
    }

    let mut spans = Vec::new();
    let mut i = 0usize;
    let mut j = 0usize;

    while i < m && j < n {
        if source_chars[i] == candidate_chars[j] {
            push_diff(&mut spans, DiffType::Unchanged, source_chars[i]);
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            push_diff(&mut spans, DiffType::Delete, source_chars[i]);
            i += 1;
        } else {
            push_diff(&mut spans, DiffType::Insert, candidate_chars[j]);
            j += 1;
        }
    }

    while i < m {
        push_diff(&mut spans, DiffType::Delete, source_chars[i]);
        i += 1;
    }

    while j < n {
        push_diff(&mut spans, DiffType::Insert, candidate_chars[j]);
        j += 1;
    }

    spans
}

fn push_diff(spans: &mut Vec<DiffSpan>, kind: DiffType, ch: char) {
    if let Some(last) = spans.last_mut() {
        if last.r#type == kind {
            last.text.push(ch);
            return;
        }
    }

    spans.push(DiffSpan {
        r#type: kind,
        text: ch.to_string(),
    });
}

fn format_reqwest_error(error: reqwest::Error) -> String {
    let mut lines = Vec::new();
    lines.push(error.to_string());

    if error.is_timeout() {
        lines.push("提示：请求超时。可以在设置里把“超时（毫秒）”调大（例如 120000）。".to_string());
    }

    if error.is_connect() {
        lines.push("提示：连接失败。常见原因：代理未生效 / DNS 异常 / 证书校验失败 / 网络被拦截。".to_string());
    }

    if error.is_request() {
        lines.push("提示：请求构造失败。请检查 Base URL 格式是否正确（建议只填根地址或 /v1）。".to_string());
    }

    if error.is_body() {
        lines.push("提示：请求体发送失败。可能是网络中断或服务端提前断开连接。".to_string());
    }

    if error.is_decode() {
        lines.push("提示：响应解码失败。可能是接口返回格式不兼容 OpenAI chat/completions。".to_string());
    }

    // 追加底层错误链，帮助定位具体原因（例如证书、DNS、连接拒绝等）
    let mut source = error.source();
    while let Some(cause) = source {
        lines.push(format!("底层错误：{cause}"));
        source = cause.source();
    }

    lines.join("\n")
}

pub fn build_client(settings: &AppSettings) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_millis(settings.timeout_ms))
        .build()
        .map_err(|error| error.to_string())
}

pub async fn test_provider(settings: &AppSettings) -> Result<ProviderCheckResult, String> {
    validate_settings(settings)?;

    let client = build_client(settings)?;

    let response = client
        .get(models_url(&settings.base_url))
        .header(AUTHORIZATION, format!("Bearer {}", settings.api_key))
        .send()
        .await
        .map_err(format_reqwest_error)?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Ok(ProviderCheckResult {
            ok: false,
            message: format!("连接失败：{} {}", status, text),
        });
    }

    Ok(ProviderCheckResult {
        ok: true,
        message: "连接测试通过，模型服务可访问。".to_string(),
    })
}

pub async fn rewrite_chunk_with_client(
    client: &reqwest::Client,
    settings: &AppSettings,
    source_text: &str,
) -> Result<String, String> {
    validate_settings(settings)?;

    let system_prompt = resolve_system_prompt(settings);
    let multiline = source_text.contains('\n') || source_text.contains('\r');
    if multiline {
        let source_lines = split_lines_keep_empty(source_text);
        if source_lines.iter().all(|line| line.trim().is_empty()) {
            // 纯空白/纯缩进：不走模型，直接原样返回。
            return Ok(source_text.to_string());
        }

        let (user_prompt, expected_lines) = build_multiline_rewrite_prompt(source_text);
        let sanitized = call_rewrite_model(client, settings, &system_prompt, &user_prompt).await?;

        let expected = source_lines.len().max(expected_lines).max(1);
        let rewritten_lines = if let Some(lines) = try_parse_multiline_rewrite_response(&sanitized, expected) {
            if blank_pattern_matches(&source_lines, &lines) {
                lines
            } else {
                split_lines_keep_empty(
                    &rewrite_multiline_fallback_per_line(
                        client,
                        settings,
                        &system_prompt,
                        &source_lines,
                    )
                    .await?,
                )
            }
        } else {
            // 如果模型不按模板输出（没有 @@@ 序号），宁可走逐行兜底，也不要冒险接收未知格式。
            let candidate_lines = split_lines_keep_empty(&sanitized);
            if candidate_lines.len() == expected
                && blank_pattern_matches(&source_lines, &candidate_lines)
                && candidate_lines.iter().all(|line| !trim_ascii_spaces_tabs_start(line).starts_with("@@@"))
            {
                candidate_lines
            } else {
                split_lines_keep_empty(
                    &rewrite_multiline_fallback_per_line(
                        client,
                        settings,
                        &system_prompt,
                        &source_lines,
                    )
                    .await?,
                )
            }
        };

        // ✅ 最关键的一步：对每一行做“格式骨架锁定”，把缩进/列表符号/编号/行尾空格等强制恢复到源文本。
        let mut enforced = Vec::with_capacity(source_lines.len());
        for (source, candidate) in source_lines.iter().zip(rewritten_lines.iter()) {
            enforced.push(enforce_line_skeleton(source, candidate));
        }

        Ok(enforced.join("\n"))
    } else {
        let (prefix, core, suffix) = split_line_skeleton(source_text);
        if core.trim().is_empty() {
            return Ok(source_text.to_string());
        }

        let user_prompt = build_singleline_rewrite_prompt(&core);
        let sanitized = call_rewrite_model(client, settings, &system_prompt, &user_prompt).await?;
        let rewritten = collapse_line_breaks_to_spaces(&sanitized);

        let body = strip_redundant_prefix(&rewritten, &prefix);
        if body.trim().is_empty() {
            return Ok(source_text.to_string());
        }

        Ok(format!("{prefix}{body}{suffix}"))
    }
}

pub async fn rewrite_chunk(settings: &AppSettings, source_text: &str) -> Result<String, String> {
    let client = build_client(settings)?;
    rewrite_chunk_with_client(&client, settings, source_text).await
}

async fn call_rewrite_model(
    client: &reqwest::Client,
    settings: &AppSettings,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<String, String> {
    let request_body = json!({
        "model": settings.model,
        "temperature": settings.temperature,
        "messages": [
            {
                "role": "system",
                "content": system_prompt
            },
            {
                "role": "user",
                "content": user_prompt
            }
        ]
    });

    let response = client
        .post(chat_url(&settings.base_url))
        .header(AUTHORIZATION, format!("Bearer {}", settings.api_key))
        .header(CONTENT_TYPE, "application/json")
        .json(&request_body)
        .send()
        .await
        .map_err(format_reqwest_error)?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("模型调用失败：{} {}", status, text));
    }

    let value: Value = response.json().await.map_err(|error| error.to_string())?;
    let content = extract_content(&value).ok_or_else(|| "模型没有返回有效文本。".to_string())?;
    let sanitized = sanitize_response(&content);

    if sanitized.is_empty() {
        return Err("模型返回内容为空。".to_string());
    }

    Ok(sanitized)
}

async fn rewrite_single_line_with_client(
    client: &reqwest::Client,
    settings: &AppSettings,
    system_prompt: &str,
    source_text: &str,
) -> Result<String, String> {
    let (prefix, core, suffix) = split_line_skeleton(source_text);
    if core.trim().is_empty() {
        return Ok(source_text.to_string());
    }

    let user_prompt = build_singleline_rewrite_prompt(&core);
    let sanitized = call_rewrite_model(client, settings, system_prompt, &user_prompt).await?;
    let rewritten = collapse_line_breaks_to_spaces(&sanitized);
    let body = strip_redundant_prefix(&rewritten, &prefix);
    if body.trim().is_empty() {
        return Ok(source_text.to_string());
    }

    Ok(format!("{prefix}{body}{suffix}"))
}

async fn rewrite_multiline_fallback_per_line(
    client: &reqwest::Client,
    settings: &AppSettings,
    system_prompt: &str,
    source_lines: &[String],
) -> Result<String, String> {
    let mut rebuilt = Vec::with_capacity(source_lines.len());
    for line in source_lines.iter() {
        if line.trim().is_empty() {
            // 空行/仅空白的行属于格式骨架：原样保留（包括可能存在的空格缩进）。
            rebuilt.push(line.to_string());
            continue;
        }

        let rewritten_line =
            rewrite_single_line_with_client(client, settings, system_prompt, line).await?;
        rebuilt.push(rewritten_line);
    }

    Ok(rebuilt.join("\n"))
}

fn build_singleline_rewrite_prompt(source_body: &str) -> String {
    format!(
        "请改写下面这段文字。保留原意与信息密度，尽量减少机械重复感。不要添加解释。\n\n格式要求（必须遵守）：\n- 严格保持原文的换行/空行/缩进/列表符号与标点风格，不要新增或删除换行。\n- 不要输出 Markdown（尤其不要使用行尾两个空格来制造换行）。\n- 只输出改写后的正文，不要输出任何标签或解释。\n\n原文：\n{}",
        source_body
    )
}

fn build_multiline_rewrite_prompt(source_body: &str) -> (String, usize) {
    // 将换行统一为 LF，保证我们构造的“逐行模板”稳定。
    let normalized = normalize_line_endings_to_lf(source_body);
    let lines = normalized.split('\n').collect::<Vec<_>>();
    let expected = lines.len().max(1);

    let mut template = String::new();
    for (index, line) in lines.iter().enumerate() {
        let number = index + 1;
        template.push_str("@@@");
        template.push_str(&number.to_string());
        template.push_str("@@@");
        template.push_str(line);
        if number < expected {
            template.push('\n');
        }
    }

    let prompt = format!(
        "请对下面的文本进行改写，让表达更自然，但必须【严格保持行结构不变】。\n\n输入格式：每行以 @@@序号@@@ 开头（序号从 1 开始）。\n输出要求（必须遵守）：\n- 必须输出【相同数量】的行，行序号必须从 1 到 {expected} 连续且不重复。\n- 每行必须保留对应的 @@@序号@@@ 前缀，且不得新增、删除、合并或拆分任何一行。\n- 每行改写后的内容必须在同一行内，不得包含换行符。\n- 空行（只有前缀没有内容）必须原样输出为空行（仍保留前缀）。\n- 不要输出 Markdown/代码块/解释/标题，只输出这些行。\n\n原文：\n{template}"
    );

    (prompt, expected)
}

fn split_lines_keep_empty(text: &str) -> Vec<String> {
    normalize_line_endings_to_lf(text)
        .split('\n')
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
}

fn blank_pattern_matches(source_lines: &[String], candidate_lines: &[String]) -> bool {
    if source_lines.len() != candidate_lines.len() {
        return false;
    }

    source_lines
        .iter()
        .zip(candidate_lines.iter())
        .all(|(source, candidate)| source.trim().is_empty() == candidate.trim().is_empty())
}

fn try_parse_multiline_rewrite_response(
    output: &str,
    expected_lines: usize,
) -> Option<Vec<String>> {
    let normalized = normalize_line_endings_to_lf(output);
    let mut collected: Vec<Option<String>> = vec![None; expected_lines];

    for raw_line in normalized.split('\n') {
        if !raw_line.starts_with("@@@") {
            continue;
        }

        let rest = &raw_line["@@@".len()..];
        let mut digits_end = 0usize;
        for (offset, ch) in rest.char_indices() {
            if ch.is_ascii_digit() {
                digits_end = offset + ch.len_utf8();
            } else {
                break;
            }
        }

        if digits_end == 0 {
            continue;
        }

        let num_str = &rest[..digits_end];
        let Ok(number) = num_str.parse::<usize>() else {
            continue;
        };
        if number == 0 || number > expected_lines {
            continue;
        }

        let after_digits = &rest[digits_end..];
        if !after_digits.starts_with("@@@") {
            continue;
        }

        let content = after_digits["@@@".len()..].to_string();
        if collected[number - 1].is_some() {
            return None;
        }
        collected[number - 1] = Some(content);
    }

    if collected.iter().all(|value| value.is_some()) {
        Some(
            collected
                .into_iter()
                .map(|value| value.unwrap_or_default())
                .collect::<Vec<_>>(),
        )
    } else {
        None
    }
}

fn validate_settings(settings: &AppSettings) -> Result<(), String> {
    if settings.base_url.trim().is_empty() {
        return Err("Base URL 不能为空。".to_string());
    }
    if settings.api_key.trim().is_empty() {
        return Err("API Key 不能为空。".to_string());
    }
    if settings.model.trim().is_empty() {
        return Err("模型名称不能为空。".to_string());
    }

    Ok(())
}

fn normalize_base_url(base_url: &str) -> String {
    base_url.trim().trim_end_matches('/').to_string()
}

fn models_url(base_url: &str) -> String {
    let normalized = normalize_base_url(base_url);
    if normalized.ends_with("/models") {
        normalized
    } else if normalized.ends_with("/v1") {
        format!("{normalized}/models")
    } else {
        format!("{normalized}/v1/models")
    }
}

fn chat_url(base_url: &str) -> String {
    let normalized = normalize_base_url(base_url);
    if normalized.ends_with("/chat/completions") {
        normalized
    } else if normalized.ends_with("/v1") {
        format!("{normalized}/chat/completions")
    } else {
        format!("{normalized}/v1/chat/completions")
    }
}

fn extract_content(value: &Value) -> Option<String> {
    let content = &value["choices"][0]["message"]["content"];

    if let Some(text) = content.as_str() {
        return Some(text.to_string());
    }

    if let Some(items) = content.as_array() {
        let merged = items
            .iter()
            .filter_map(|item| item["text"].as_str())
            .collect::<Vec<_>>()
            .join("");
        if !merged.is_empty() {
            return Some(merged);
        }
    }

    None
}

fn sanitize_response(content: &str) -> String {
    let trimmed = content.trim();
    let without_fences = if trimmed.starts_with("```") {
        trimmed
            .lines()
            .filter(|line| !line.trim_start().starts_with("```"))
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string()
    } else {
        trimmed.to_string()
    };

    // 一些提示词会诱导模型输出“修改后：...”或类似标签；这里做一次轻量清理，
    // 避免影响 diff 与导出。
    let mut cleaned = without_fences.trim().to_string();
    for prefix in ["修改后：", "修改后:", "改写后：", "改写后:", "润色后：", "润色后:"] {
        if cleaned.starts_with(prefix) {
            cleaned = cleaned[prefix.len()..].trim_start().to_string();
        }
    }
    if cleaned.starts_with("修改后") {
        let after = cleaned["修改后".len()..].trim_start();
        if let Some(first) = after.chars().next() {
            if matches!(first, ':' | '：' | '-' | '—') {
                cleaned = after[first.len_utf8()..].trim_start().to_string();
            } else if first == '\n' {
                cleaned = after.trim_start().to_string();
            }
        }
    }

    cleaned
}

#[cfg(test)]
mod tests {
    use super::{
        build_diff, convert_line_endings, detect_line_ending, has_trailing_spaces_per_line,
        normalize_text, segment_text, strip_trailing_spaces_per_line, LineEnding,
    };
    use crate::models::ChunkPreset;

    #[test]
    fn normalizes_line_endings_and_blank_lines() {
        let input = "第一段\r\n\r\n\r\n 第二段 \r\n";
        assert_eq!(normalize_text(input), "第一段\n\n第二段");
    }

    #[test]
    fn segments_long_paragraphs() {
        let text = "这是第一句。".repeat(80);
        let chunks = segment_text(&text, ChunkPreset::Clause);
        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|chunk| !chunk.text.is_empty()));
    }

    #[test]
    fn keeps_paragraph_separator_when_splitting_by_sentence() {
        let text = "第一句。第二句。\n\n第三句。";
        let chunks = segment_text(text, ChunkPreset::Sentence);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].separator_after, "");
        assert_eq!(chunks[1].separator_after, "\n\n");
        assert_eq!(chunks[2].separator_after, "");
    }

    #[test]
    fn builds_inline_diff() {
        let spans = build_diff("你好", "hollow");
        assert!(spans.iter().any(|span| span.text.contains('你')));
        assert!(spans.iter().any(|span| span.text.contains('h')));
    }

    #[test]
    fn detects_line_endings() {
        assert_eq!(detect_line_ending("a\r\nb\r\n"), LineEnding::CrLf);
        assert_eq!(detect_line_ending("a\nb\n"), LineEnding::Lf);
        assert_eq!(detect_line_ending("a\rb\r"), LineEnding::Cr);
        assert_eq!(detect_line_ending("single line"), LineEnding::None);
    }

    #[test]
    fn converts_line_endings_without_doubling() {
        let input = "a\r\nb\nc\rd";
        let converted = convert_line_endings(input, LineEnding::CrLf);
        assert_eq!(converted, "a\r\nb\r\nc\r\nd");
    }

    #[test]
    fn strips_trailing_spaces_per_line_preserving_endings() {
        let input = "a  \r\nb\t\nc \rd";
        let stripped = strip_trailing_spaces_per_line(input);
        assert_eq!(stripped, "a\r\nb\nc\rd");
        assert!(has_trailing_spaces_per_line(input));
        assert!(!has_trailing_spaces_per_line(&stripped));
    }

    #[test]
    fn segments_preserve_newlines_in_separators() {
        let text = "第一行\r\n第二行\n第三行\r第四行";
        let chunks = segment_text(text, ChunkPreset::Clause);
        assert!(chunks
            .iter()
            .all(|chunk| !chunk.text.contains('\n') && !chunk.text.contains('\r')));

        let rebuilt = chunks
            .iter()
            .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
            .collect::<String>();
        assert_eq!(rebuilt, text);
    }
}
