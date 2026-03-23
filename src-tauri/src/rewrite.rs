use std::time::Duration;

use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::Deserialize;
use serde_json::{json, Value};
use std::error::Error as _;

use crate::adapters::markdown::MarkdownAdapter;
use crate::adapters::tex::TexAdapter;
use crate::adapters::TextRegion;
use crate::models::{
    AppSettings, ChunkPreset, DiffSpan, DiffType, ProviderCheckResult, SegmentationMode,
};

const SYSTEM_PROMPT_FALLBACK: &str = "你是一名严谨的中文文本编辑。你的任务是对给定片段进行自然化改写，让表达更像真实人工写作，但必须保持原意、事实、语气和段落层次稳定。不要扩写，不要总结，不要解释，不要输出标题，只输出改写后的正文。";
const SYSTEM_PROMPT_AIGC_V1: &str = include_str!("../../prompt/1.txt");
const SYSTEM_PROMPT_HUMANIZER_ZH: &str = include_str!("../../prompt/2.txt");
const AI_SEGMENTATION_SYSTEM_PROMPT: &str = "你是一名文本分块规划器。你的任务不是改写文本，而是把输入的一组连续原子片段按索引重新分组，供后续润色模型逐组处理。\n\n硬性约束：\n1. 绝对不要改写、删减、补充、重排任何正文。\n2. 只能返回 JSON，格式必须是 {\"groups\":[[0,1],[2],[3,4]]}。\n3. 每个索引必须且只能出现一次，必须保持原始顺序，组内索引必须连续。\n4. 优先在语义自然边界处分组，避免把很短的引导语单独成块。\n5. 单组长度尽量不要超过给定上限。\n6. 不要输出解释、标题、Markdown 代码块。";

const MAX_CLAUSE_CHARS: usize = 420;
const MAX_SENTENCE_CHARS: usize = 900;
const MAX_PARAGRAPH_CHARS: usize = 1_600;
const MERGE_TINY_CLAUSE_CHARS: usize = 12;
const AI_SEGMENTATION_MIN_TEXT_CHARS: usize = 320;
const AI_SEGMENTATION_MAX_WINDOW_CHARS: usize = 4_200;
const AI_SEGMENTATION_MAX_ATOMS: usize = 72;
const AI_SEGMENTATION_TINY_CHUNK_CHARS: usize = 12;

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
    let base = if base.is_empty() {
        SYSTEM_PROMPT_FALLBACK
    } else {
        base
    };

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
    /// 是否跳过改写（例如 Markdown fenced code block）。
    ///
    /// 设计动机：
    /// - 代码块/配置片段属于“格式/语义强约束内容”，让模型改写极易改坏；
    /// - 与其提示模型“不要改”，不如直接跳过，保持原样。
    pub skip_rewrite: bool,
}

#[derive(Debug, Clone)]
struct ParagraphBlock {
    body: String,
    separator_after: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct SegmentDiagnostics {
    rewriteable_chunks: usize,
    rewriteable_chars: usize,
    tiny_chunks: usize,
    forced_splits: usize,
}

#[derive(Debug, Clone)]
struct AiSegmentationWindow {
    start: usize,
    end: usize,
    diagnostics: SegmentDiagnostics,
}

#[derive(Debug, Deserialize)]
struct AiSegmentationResponse {
    groups: Vec<Vec<usize>>,
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

fn normalize_line_endings_to_lf(text: &str) -> String {
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

fn chunk_char_limit(preset: ChunkPreset) -> usize {
    match preset {
        ChunkPreset::Clause => MAX_CLAUSE_CHARS,
        ChunkPreset::Sentence => MAX_SENTENCE_CHARS,
        ChunkPreset::Paragraph => MAX_PARAGRAPH_CHARS,
    }
}

fn ai_atom_char_limit(preset: ChunkPreset) -> usize {
    match preset {
        ChunkPreset::Clause => 80,
        ChunkPreset::Sentence => 120,
        ChunkPreset::Paragraph => 160,
    }
}

fn rebuild_chunks_text(chunks: &[SegmentedChunk]) -> String {
    chunks
        .iter()
        .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
        .collect::<String>()
}

fn is_natural_boundary_char(ch: char) -> bool {
    is_closing_punctuation(ch)
        || matches!(
            ch,
            '。' | '！' | '？' | '!' | '?' | '；' | ';' | '，' | '、' | '：' | ':' | ','
        )
}

fn looks_like_forced_split(text: &str, preset: ChunkPreset) -> bool {
    let trimmed = text.trim_end();
    if trimmed.is_empty() {
        return false;
    }

    let len = trimmed.chars().count();
    let limit = chunk_char_limit(preset);
    if len + 48 < limit {
        return false;
    }

    trimmed
        .chars()
        .last()
        .map(|ch| !is_natural_boundary_char(ch))
        .unwrap_or(false)
}

fn analyze_chunk_window(chunks: &[SegmentedChunk], preset: ChunkPreset) -> SegmentDiagnostics {
    let mut diagnostics = SegmentDiagnostics::default();

    for chunk in chunks.iter().filter(|chunk| !chunk.skip_rewrite) {
        diagnostics.rewriteable_chunks = diagnostics.rewriteable_chunks.saturating_add(1);
        let len = chunk.text.chars().count();
        diagnostics.rewriteable_chars = diagnostics.rewriteable_chars.saturating_add(len);

        if len > 0 && len <= AI_SEGMENTATION_TINY_CHUNK_CHARS {
            diagnostics.tiny_chunks = diagnostics.tiny_chunks.saturating_add(1);
        }
        if looks_like_forced_split(&chunk.text, preset) {
            diagnostics.forced_splits = diagnostics.forced_splits.saturating_add(1);
        }
    }

    diagnostics
}

fn should_try_ai_for_window(diagnostics: &SegmentDiagnostics) -> bool {
    if diagnostics.rewriteable_chunks < 4
        || diagnostics.rewriteable_chars < AI_SEGMENTATION_MIN_TEXT_CHARS
    {
        return false;
    }

    if diagnostics.forced_splits >= 2 {
        return true;
    }

    diagnostics.rewriteable_chunks >= 8
        && diagnostics.tiny_chunks >= 3
        && diagnostics.tiny_chunks * 100 >= diagnostics.rewriteable_chunks * 35
}

fn collect_ai_segmentation_windows(
    chunks: &[SegmentedChunk],
    preset: ChunkPreset,
) -> Vec<AiSegmentationWindow> {
    let mut windows = Vec::new();
    let mut start: Option<usize> = None;

    for (index, chunk) in chunks.iter().enumerate() {
        if chunk.skip_rewrite {
            if let Some(window_start) = start.take() {
                let diagnostics = analyze_chunk_window(&chunks[window_start..index], preset);
                if should_try_ai_for_window(&diagnostics) {
                    windows.push(AiSegmentationWindow {
                        start: window_start,
                        end: index,
                        diagnostics,
                    });
                }
            }
            continue;
        }

        if start.is_none() {
            start = Some(index);
        }
    }

    if let Some(window_start) = start {
        let diagnostics = analyze_chunk_window(&chunks[window_start..], preset);
        if should_try_ai_for_window(&diagnostics) {
            windows.push(AiSegmentationWindow {
                start: window_start,
                end: chunks.len(),
                diagnostics,
            });
        }
    }

    windows
}

fn build_ai_segmentation_atoms(text: &str, preset: ChunkPreset) -> Vec<SegmentedChunk> {
    let atoms = segment_by_boundary(text, BoundaryKind::Clause, ai_atom_char_limit(preset));
    if atoms.is_empty() {
        vec![SegmentedChunk {
            text: text.to_string(),
            separator_after: String::new(),
            skip_rewrite: false,
        }]
    } else {
        atoms
    }
}

fn validate_ai_groups(groups: &[Vec<usize>], atoms_len: usize) -> Result<(), String> {
    if atoms_len == 0 {
        if groups.is_empty() {
            return Ok(());
        }
        return Err("AI 分块返回了多余分组。".to_string());
    }

    if groups.is_empty() {
        return Err("AI 分块没有返回任何分组。".to_string());
    }

    let mut expected = 0usize;
    for group in groups {
        if group.is_empty() {
            return Err("AI 分块返回了空分组。".to_string());
        }

        for (position, index) in group.iter().enumerate() {
            if *index != expected {
                return Err("AI 分块索引不连续或顺序错误。".to_string());
            }
            if position > 0 && *index != group[position - 1].saturating_add(1) {
                return Err("AI 分块组内索引必须连续。".to_string());
            }
            expected = expected.saturating_add(1);
        }
    }

    if expected != atoms_len {
        return Err("AI 分块没有完整覆盖所有原子片段。".to_string());
    }

    Ok(())
}

fn build_chunks_from_ai_groups(
    atoms: &[SegmentedChunk],
    groups: &[Vec<usize>],
) -> Vec<SegmentedChunk> {
    let mut chunks = Vec::with_capacity(groups.len());

    for group in groups {
        let last = *group.last().unwrap_or(&0);
        let mut text = String::new();

        for (position, index) in group.iter().enumerate() {
            let atom = &atoms[*index];
            text.push_str(&atom.text);
            if position + 1 < group.len() {
                text.push_str(&atom.separator_after);
            }
        }

        chunks.push(SegmentedChunk {
            text,
            separator_after: atoms[last].separator_after.clone(),
            skip_rewrite: false,
        });
    }

    chunks
}

fn build_ai_segmentation_prompt(
    atoms: &[SegmentedChunk],
    preset: ChunkPreset,
    max_chars: usize,
) -> String {
    let atoms_payload = atoms
        .iter()
        .enumerate()
        .map(|(index, atom)| {
            json!({
                "id": index,
                "text": atom.text,
                "separatorAfter": atom.separator_after,
                "charCount": atom.text.chars().count(),
                "hasLineBreakAfter": atom.separator_after.contains('\n') || atom.separator_after.contains('\r'),
            })
        })
        .collect::<Vec<_>>();

    let granularity = match preset {
        ChunkPreset::Clause => "当前默认粒度偏细，优先合并被规则切碎的相邻原子。",
        ChunkPreset::Sentence => "当前默认粒度为整句，优先保持句子级完整性。",
        ChunkPreset::Paragraph => {
            "当前默认粒度为段落，优先保持自然段内部语义完整，但不要把过长段落整块塞回去。"
        }
    };

    format!(
        "请根据下面的连续原子片段规划分组。\n\n目标：每组尽量语义完整，且单组正文长度不超过 {max_chars} 字。\n{granularity}\n\n只返回 JSON，不要附带解释。\n\n原子列表：\n{}",
        serde_json::to_string_pretty(&atoms_payload).unwrap_or_else(|_| "[]".to_string())
    )
}

async fn request_ai_segmentation_groups(
    client: &reqwest::Client,
    settings: &AppSettings,
    atoms: &[SegmentedChunk],
    preset: ChunkPreset,
) -> Result<Vec<Vec<usize>>, String> {
    let prompt = build_ai_segmentation_prompt(atoms, preset, chunk_char_limit(preset));
    let raw = call_json_model(
        client,
        settings,
        AI_SEGMENTATION_SYSTEM_PROMPT,
        &prompt,
        0.1,
    )
    .await?;

    let parsed: AiSegmentationResponse =
        serde_json::from_str(&raw).map_err(|error| format!("AI 分块 JSON 解析失败：{error}"))?;
    validate_ai_groups(&parsed.groups, atoms.len())?;

    let grouped = build_chunks_from_ai_groups(atoms, &parsed.groups);
    if grouped
        .iter()
        .any(|chunk| chunk.text.chars().count() > chunk_char_limit(preset))
    {
        return Err("AI 分块返回了超出长度上限的分组。".to_string());
    }

    let rebuilt = rebuild_chunks_text(&grouped);
    let original = rebuild_chunks_text(atoms);
    if rebuilt != original {
        return Err("AI 分块校验失败：拼接结果与原文不一致。".to_string());
    }

    Ok(parsed.groups)
}

async fn resegment_chunks_with_ai(
    client: &reqwest::Client,
    settings: &AppSettings,
    chunks: &[SegmentedChunk],
    preset: ChunkPreset,
) -> Result<Option<Vec<SegmentedChunk>>, String> {
    let windows = collect_ai_segmentation_windows(chunks, preset);
    if windows.is_empty() {
        return Ok(None);
    }

    let original_text = rebuild_chunks_text(chunks);
    let mut out = Vec::new();
    let mut cursor = 0usize;
    let mut changed = false;

    for window in windows {
        out.extend(chunks[cursor..window.start].iter().cloned());

        let original_window = &chunks[window.start..window.end];
        let window_text = rebuild_chunks_text(original_window);
        if window.diagnostics.rewriteable_chars > AI_SEGMENTATION_MAX_WINDOW_CHARS {
            out.extend(original_window.iter().cloned());
            cursor = window.end;
            continue;
        }

        let atoms = build_ai_segmentation_atoms(&window_text, preset);
        if atoms.len() < 2 || atoms.len() > AI_SEGMENTATION_MAX_ATOMS {
            out.extend(original_window.iter().cloned());
            cursor = window.end;
            continue;
        }

        match request_ai_segmentation_groups(client, settings, &atoms, preset).await {
            Ok(groups) => {
                let grouped = build_chunks_from_ai_groups(&atoms, &groups);
                if grouped.as_slice() != original_window {
                    changed = true;
                }
                out.extend(grouped);
            }
            Err(_) => out.extend(original_window.iter().cloned()),
        }

        cursor = window.end;
    }

    out.extend(chunks[cursor..].iter().cloned());

    if !changed || rebuild_chunks_text(&out) != original_text {
        return Ok(None);
    }

    Ok(Some(out))
}

pub async fn segment_text_for_rewrite(
    settings: &AppSettings,
    text: &str,
    preset: ChunkPreset,
) -> Result<Vec<SegmentedChunk>, String> {
    let chunks = segment_text(text, preset);
    if settings.segmentation_mode != SegmentationMode::AiFallback {
        return Ok(chunks);
    }

    let client = build_client(settings)?;
    match resegment_chunks_with_ai(&client, settings, &chunks, preset).await {
        Ok(Some(grouped)) => Ok(grouped),
        Ok(None) => Ok(chunks),
        Err(_) => Ok(chunks),
    }
}

pub fn segment_text(text: &str, preset: ChunkPreset) -> Vec<SegmentedChunk> {
    let enable_markdown = MarkdownAdapter::should_adapt(text);
    let enable_tex = TexAdapter::should_adapt(text);

    // 没有任何适配器需求：走纯文本快速路径。
    if !enable_markdown && !enable_tex {
        return segment_plain_text(text, preset);
    }

    // 适配器管线：
    // - Markdown：先跳过 fenced code/table/front matter 等结构块
    // - TeX：再跳过数学/命令/注释/代码环境等语法强约束片段
    let mut regions = if enable_markdown {
        MarkdownAdapter::split_regions(text)
    } else {
        vec![TextRegion {
            body: text.to_string(),
            skip_rewrite: false,
        }]
    };

    if enable_tex {
        regions = split_regions_with_tex(regions);
    }

    if regions.len() == 1 && !regions[0].skip_rewrite {
        return segment_plain_text(text, preset);
    }

    let mut chunks: Vec<SegmentedChunk> = Vec::new();
    for region in regions.into_iter() {
        if region.body.is_empty() {
            continue;
        }

        if region.skip_rewrite {
            append_raw_chunk(&mut chunks, &region.body, true);
            continue;
        }

        let mut pieces = segment_plain_text(&region.body, preset);
        if !chunks.is_empty() && !pieces.is_empty() && pieces[0].text.is_empty() {
            let leading = pieces.remove(0).separator_after;
            if !leading.is_empty() {
                if let Some(last) = chunks.last_mut() {
                    last.separator_after.push_str(&leading);
                }
            }
        }
        chunks.extend(pieces);
    }

    if chunks.is_empty() {
        vec![SegmentedChunk {
            text: text.to_string(),
            separator_after: String::new(),
            skip_rewrite: false,
        }]
    } else {
        chunks
    }
}

fn split_regions_with_tex(regions: Vec<TextRegion>) -> Vec<TextRegion> {
    let mut out: Vec<TextRegion> = Vec::new();

    for region in regions.into_iter() {
        if region.body.is_empty() {
            continue;
        }

        if region.skip_rewrite {
            push_text_region(&mut out, region);
            continue;
        }

        let sub = TexAdapter::split_regions(&region.body);
        for item in sub.into_iter() {
            push_text_region(&mut out, item);
        }
    }

    out
}

fn push_text_region(regions: &mut Vec<TextRegion>, region: TextRegion) {
    if region.body.is_empty() {
        return;
    }

    if let Some(last) = regions.last_mut() {
        if last.skip_rewrite == region.skip_rewrite {
            last.body.push_str(&region.body);
            return;
        }
    }

    regions.push(region);
}

fn append_raw_chunk(chunks: &mut Vec<SegmentedChunk>, text: &str, skip_rewrite: bool) {
    let (body, trailing_ws) = split_trailing_whitespace(text);
    if body.is_empty() {
        append_separator_to_last(chunks, trailing_ws);
        return;
    }

    chunks.push(SegmentedChunk {
        text: body,
        separator_after: trailing_ws,
        skip_rewrite,
    });
}

fn segment_plain_text(text: &str, preset: ChunkPreset) -> Vec<SegmentedChunk> {
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
    let blocks = split_paragraph_blocks(text);

    let mut chunks: Vec<SegmentedChunk> = Vec::new();

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
                        skip_rewrite: false,
                    });
                    continue;
                }

                let mut pieces = segment_by_sentence(&body, MAX_SENTENCE_CHARS, MAX_CLAUSE_CHARS);
                append_separator_to_last(&mut pieces, paragraph_separator);
                chunks.extend(pieces);
            }
            ChunkPreset::Sentence => {
                let mut pieces = segment_by_sentence(&body, MAX_SENTENCE_CHARS, MAX_CLAUSE_CHARS);
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

    // 小句合并（可选）：避免出现大量“超短引导语”导致 chunk 过碎。
    // 规则尽量保守：
    // - 只在 separator_after 为空时合并（避免把换行/空格注入模型输入造成格式漂移）
    // - 仅合并以弱分隔符结尾的超短片段（例如 “此外，” “因此，”）
    //
    // 目的：减少碎片化，提高连贯性与吞吐，同时不影响格式保真。
    if preset != ChunkPreset::Paragraph && chunks.len() >= 2 {
        let max_chars = match preset {
            ChunkPreset::Clause => MAX_CLAUSE_CHARS,
            ChunkPreset::Sentence => MAX_SENTENCE_CHARS,
            ChunkPreset::Paragraph => MAX_PARAGRAPH_CHARS,
        };

        let is_weak_boundary = |ch: char| matches!(ch, '，' | '、' | '：' | ':' | '；' | ';' | ',');

        let mut index = 0usize;
        while index + 1 < chunks.len() {
            let current = &chunks[index];
            let next = &chunks[index + 1];

            let can_merge = !current.skip_rewrite
                && !next.skip_rewrite
                && current.separator_after.is_empty()
                && current.text.chars().count() > 0
                && current.text.chars().count() <= MERGE_TINY_CLAUSE_CHARS
                && current
                    .text
                    .chars()
                    .last()
                    .map(is_weak_boundary)
                    .unwrap_or(false)
                && (current.text.chars().count() + next.text.chars().count() <= max_chars);

            if !can_merge {
                index += 1;
                continue;
            }

            let prefix = chunks[index].text.clone();
            chunks[index + 1].text = format!("{prefix}{}", chunks[index + 1].text);
            chunks.remove(index);
            if index > 0 {
                index -= 1;
            }
        }
    }

    if chunks.is_empty() {
        vec![SegmentedChunk {
            text: text.to_string(),
            separator_after: String::new(),
            skip_rewrite: false,
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

        if is_blank {
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
            // 纯分隔符 chunk（例如文件头部的空行/空白行）。
            // 不应进入重写队列，否则会导致无意义调用或格式抖动风险。
            skip_rewrite: true,
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

        let mut pieces =
            segment_by_boundary(&sentence.text, BoundaryKind::Clause, max_sentence_chars);
        append_separator_to_last(&mut pieces, sentence.separator_after);

        // Clause 依然可能出现极端长段（没有任何标点），这里再加一个硬上限兜底。
        if max_clause_chars > 0 {
            let mut bounded = Vec::new();
            for piece in pieces.into_iter() {
                if piece.text.chars().count() <= max_sentence_chars {
                    bounded.push(piece);
                    continue;
                }
                let mut hard =
                    segment_by_boundary(&piece.text, BoundaryKind::Clause, max_sentence_chars);
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
                    skip_rewrite: false,
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
                skip_rewrite: false,
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
        '"' | '\'' | '”' | '’' | '）' | ')' | '】' | ']' | '}' | '」' | '』' | '》' | '〉'
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
        lines.push(
            "提示：连接失败。常见原因：代理未生效 / DNS 异常 / 证书校验失败 / 网络被拦截。"
                .to_string(),
        );
    }

    if error.is_request() {
        lines.push(
            "提示：请求构造失败。请检查 Base URL 格式是否正确（建议只填根地址或 /v1）。"
                .to_string(),
        );
    }

    if error.is_body() {
        lines.push("提示：请求体发送失败。可能是网络中断或服务端提前断开连接。".to_string());
    }

    if error.is_decode() {
        lines.push(
            "提示：响应解码失败。可能是接口返回格式不兼容 OpenAI chat/completions。".to_string(),
        );
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
    let probe = call_chat_model(&client, settings, "你是连通性探针。只回复 OK。", "OK", 0.0).await;

    if let Err(error) = probe {
        return Ok(ProviderCheckResult {
            ok: false,
            message: format!("chat/completions 调用失败：{error}"),
        });
    }

    Ok(ProviderCheckResult {
        ok: true,
        message: "连接测试通过，chat/completions 可访问。".to_string(),
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
        let rewritten_lines =
            if let Some(lines) = try_parse_multiline_rewrite_response(&sanitized, expected) {
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
                    && candidate_lines
                        .iter()
                        .all(|line| !trim_ascii_spaces_tabs_start(line).starts_with("@@@"))
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
    call_chat_model(
        client,
        settings,
        system_prompt,
        user_prompt,
        settings.temperature,
    )
    .await
}

async fn call_json_model(
    client: &reqwest::Client,
    settings: &AppSettings,
    system_prompt: &str,
    user_prompt: &str,
    temperature: f32,
) -> Result<String, String> {
    call_chat_model(client, settings, system_prompt, user_prompt, temperature).await
}

async fn call_chat_model(
    client: &reqwest::Client,
    settings: &AppSettings,
    system_prompt: &str,
    user_prompt: &str,
    temperature: f32,
) -> Result<String, String> {
    let response = send_chat_request(
        client,
        settings,
        system_prompt,
        user_prompt,
        temperature,
        false,
    )
    .await?;

    let status = response.status();
    if status.is_success() {
        return parse_json_chat_response(response).await;
    }

    let body = response.text().await.unwrap_or_default();
    if response_requires_stream(status, &body) {
        return call_stream_chat_model(client, settings, system_prompt, user_prompt, temperature)
            .await;
    }

    Err(format_chat_api_error(status, &body))
}

async fn call_stream_chat_model(
    client: &reqwest::Client,
    settings: &AppSettings,
    system_prompt: &str,
    user_prompt: &str,
    temperature: f32,
) -> Result<String, String> {
    let response = send_chat_request(
        client,
        settings,
        system_prompt,
        user_prompt,
        temperature,
        true,
    )
    .await?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format_reqwest_error(error))?;

    if !status.is_success() {
        return Err(format_chat_api_error(status, &body));
    }

    parse_stream_chat_response_body(&body)
}

async fn send_chat_request(
    client: &reqwest::Client,
    settings: &AppSettings,
    system_prompt: &str,
    user_prompt: &str,
    temperature: f32,
    stream: bool,
) -> Result<reqwest::Response, String> {
    let mut request_body = json!({
        "model": settings.model,
        "temperature": temperature,
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
    if stream {
        request_body["stream"] = Value::Bool(true);
    }

    client
        .post(chat_url(&settings.base_url))
        .header(AUTHORIZATION, format!("Bearer {}", settings.api_key))
        .header(CONTENT_TYPE, "application/json")
        .json(&request_body)
        .send()
        .await
        .map_err(format_reqwest_error)
}

async fn parse_json_chat_response(response: reqwest::Response) -> Result<String, String> {
    let value: Value = response.json().await.map_err(|error| error.to_string())?;
    sanitize_completion_text(extract_content(&value))
}

fn parse_stream_chat_response_body(body: &str) -> Result<String, String> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return Err("模型返回内容为空。".to_string());
    }

    if trimmed.starts_with('{') {
        let value: Value =
            serde_json::from_str(trimmed).map_err(|error| format!("流式响应解析失败：{error}"))?;
        return sanitize_completion_text(extract_content(&value));
    }

    let mut merged = String::new();
    let mut saw_data = false;

    for raw_line in body.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        if !line.starts_with("data:") {
            continue;
        }

        saw_data = true;
        let payload = line["data:".len()..].trim();
        if payload.is_empty() || payload == "[DONE]" {
            continue;
        }

        let value: Value =
            serde_json::from_str(payload).map_err(|error| format!("流式响应解析失败：{error}"))?;
        if let Some(delta) = extract_stream_content(&value) {
            merged.push_str(&delta);
        }
    }

    if !saw_data {
        return sanitize_completion_text(Some(trimmed.to_string()));
    }

    sanitize_completion_text(Some(merged))
}

fn response_requires_stream(status: reqwest::StatusCode, body: &str) -> bool {
    if status != reqwest::StatusCode::BAD_REQUEST
        && status != reqwest::StatusCode::UNPROCESSABLE_ENTITY
    {
        return false;
    }

    let normalized = body.to_ascii_lowercase();
    normalized.contains("stream must be set to true")
        || (normalized.contains("\"param\":\"stream\"")
            && normalized.contains("must be set to true"))
        || (normalized.contains("\"stream\"") && normalized.contains("set to true"))
}

fn format_chat_api_error(status: reqwest::StatusCode, body: &str) -> String {
    let detail = extract_api_error_message(body)
        .or_else(|| {
            let trimmed = body.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        })
        .unwrap_or_default();

    if detail.is_empty() {
        format!("模型调用失败：{status}")
    } else {
        format!("模型调用失败：{status} {detail}")
    }
}

fn extract_api_error_message(body: &str) -> Option<String> {
    let value: Value = serde_json::from_str(body).ok()?;

    value["error"]["message"]
        .as_str()
        .or_else(|| value["message"].as_str())
        .map(|message| message.trim().to_string())
        .filter(|message| !message.is_empty())
}

fn sanitize_completion_text(content: Option<String>) -> Result<String, String> {
    let Some(content) = content else {
        return Err("模型没有返回有效文本。".to_string());
    };

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
    extract_text_field(&value["choices"][0]["message"]["content"])
        .or_else(|| extract_text_field(&value["choices"][0]["delta"]["content"]))
        .or_else(|| {
            value["choices"][0]["text"]
                .as_str()
                .map(|text| text.to_string())
        })
}

fn extract_stream_content(value: &Value) -> Option<String> {
    extract_text_field(&value["choices"][0]["delta"]["content"])
        .or_else(|| {
            value["choices"][0]["delta"]["text"]
                .as_str()
                .map(|text| text.to_string())
        })
        .or_else(|| extract_text_field(&value["choices"][0]["message"]["content"]))
        .or_else(|| {
            value["choices"][0]["text"]
                .as_str()
                .map(|text| text.to_string())
        })
}

fn extract_text_field(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        return Some(text.to_string());
    }

    if let Some(items) = value.as_array() {
        let merged = items
            .iter()
            .filter_map(|item| {
                item["text"]
                    .as_str()
                    .or_else(|| item["content"].as_str())
                    .or_else(|| item["value"].as_str())
            })
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
    for prefix in [
        "修改后：",
        "修改后:",
        "改写后：",
        "改写后:",
        "润色后：",
        "润色后:",
    ] {
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
        build_chunks_from_ai_groups, build_diff, collect_ai_segmentation_windows,
        convert_line_endings, detect_line_ending, extract_api_error_message,
        has_trailing_spaces_per_line, normalize_text, parse_stream_chat_response_body,
        response_requires_stream, segment_text, strip_trailing_spaces_per_line, validate_ai_groups,
        LineEnding, SegmentedChunk,
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

    #[test]
    fn marks_markdown_fenced_code_blocks_as_skip_rewrite() {
        let text = "前文。\n\n```rust\nfn main() {\n  println!(\"hi\");\n}\n```\n\n后文。";
        let chunks = segment_text(text, ChunkPreset::Sentence);
        assert!(chunks.iter().any(|chunk| chunk.skip_rewrite));

        let rebuilt = chunks
            .iter()
            .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
            .collect::<String>();
        assert_eq!(rebuilt, text);

        let code_chunk = chunks.iter().find(|chunk| chunk.skip_rewrite).unwrap();
        assert!(code_chunk.text.contains("```rust"));
        assert!(code_chunk.text.contains("fn main()"));
    }

    #[test]
    fn merges_tiny_leading_clauses_to_reduce_fragmentation() {
        let text = "此外，本文提出一种方法，效果很好。";
        let chunks = segment_text(text, ChunkPreset::Clause);
        assert!(!chunks.iter().any(|chunk| chunk.text == "此外，"));

        let rebuilt = chunks
            .iter()
            .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
            .collect::<String>();
        assert_eq!(rebuilt, text);
    }

    #[test]
    fn paragraph_preset_keeps_leading_blank_lines_out_of_first_chunk() {
        // 典型场景：文件最开头有空行（可能包含空格），不应被并入第一段正文 chunk；
        // 否则在 Paragraph 预设下，会出现“空白行 + 下一行正文”被当成一个分块的现象。
        let text = " \n第一段。\n\n第二段。";
        let chunks = segment_text(text, ChunkPreset::Paragraph);

        let rebuilt = chunks
            .iter()
            .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
            .collect::<String>();
        assert_eq!(rebuilt, text);

        assert!(chunks.len() >= 3);
        assert_eq!(chunks[0].text, "");
        assert!(chunks[0].skip_rewrite);
        assert_eq!(chunks[0].separator_after, " \n");

        assert_eq!(chunks[1].text, "第一段。");
        assert!(!chunks[1].skip_rewrite);
        assert_eq!(chunks[1].separator_after, "\n\n");
        assert_eq!(chunks[2].text, "第二段。");
    }

    #[test]
    fn marks_yaml_front_matter_as_skip_rewrite() {
        let text = "---\ntitle: 示例\ntags: [a, b]\n---\n\n正文第一句。";
        let chunks = segment_text(text, ChunkPreset::Sentence);
        assert!(chunks
            .iter()
            .any(|chunk| chunk.skip_rewrite && chunk.text.contains("title: 示例")));

        let rebuilt = chunks
            .iter()
            .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
            .collect::<String>();
        assert_eq!(rebuilt, text);
    }

    #[test]
    fn does_not_treat_lonely_horizontal_rule_as_front_matter() {
        // 只有单个 `---` 行且没有闭合 `---`/`...`：更像是 Markdown 水平线，不应把整篇当作 front matter 跳过。
        let text = "---\n\n正文。";
        let chunks = segment_text(text, ChunkPreset::Sentence);
        assert!(chunks.iter().all(|chunk| !chunk.skip_rewrite));

        let rebuilt = chunks
            .iter()
            .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
            .collect::<String>();
        assert_eq!(rebuilt, text);
    }

    #[test]
    fn marks_markdown_tables_as_skip_rewrite() {
        let text = "前文。\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n后文。";
        let chunks = segment_text(text, ChunkPreset::Sentence);
        assert!(chunks.iter().any(|chunk| {
            chunk.skip_rewrite
                && chunk.text.contains("|---|---|")
                && chunk.text.contains("| 1 | 2 |")
        }));

        let rebuilt = chunks
            .iter()
            .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
            .collect::<String>();
        assert_eq!(rebuilt, text);
    }

    #[test]
    fn marks_markdown_inline_code_as_skip_rewrite() {
        let text = "前文 `let x = 1;` 后文。";
        let chunks = segment_text(text, ChunkPreset::Sentence);
        assert!(chunks
            .iter()
            .any(|chunk| chunk.skip_rewrite && chunk.text.contains("`let x = 1;`")));

        let rebuilt = chunks
            .iter()
            .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
            .collect::<String>();
        assert_eq!(rebuilt, text);
    }

    #[test]
    fn marks_markdown_inline_links_as_skip_rewrite() {
        let text = "见 [OpenAI](https://openai.com) 的文档。";
        let chunks = segment_text(text, ChunkPreset::Sentence);
        assert!(chunks.iter().any(|chunk| {
            chunk.skip_rewrite && chunk.text.contains("[OpenAI](https://openai.com)")
        }));

        let rebuilt = chunks
            .iter()
            .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
            .collect::<String>();
        assert_eq!(rebuilt, text);
    }

    #[test]
    fn marks_markdown_reference_definitions_as_skip_rewrite() {
        let text = "[id]: https://example.com\n\n正文第一句。";
        let chunks = segment_text(text, ChunkPreset::Sentence);
        assert!(chunks.iter().any(|chunk| {
            chunk.skip_rewrite && chunk.text.contains("[id]: https://example.com")
        }));

        let rebuilt = chunks
            .iter()
            .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
            .collect::<String>();
        assert_eq!(rebuilt, text);
    }

    #[test]
    fn marks_markdown_indented_code_blocks_as_skip_rewrite() {
        let text = "前文。\n\n    fn main() {}\n    println!(\"hi\");\n\n后文。";
        let chunks = segment_text(text, ChunkPreset::Sentence);
        assert!(chunks
            .iter()
            .any(|chunk| chunk.skip_rewrite && chunk.text.contains("fn main()")));

        let rebuilt = chunks
            .iter()
            .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
            .collect::<String>();
        assert_eq!(rebuilt, text);
    }

    #[test]
    fn marks_markdown_list_prefix_as_skip_rewrite() {
        let text = "- 第一条\n- 第二条";
        let chunks = segment_text(text, ChunkPreset::Sentence);
        assert!(chunks
            .iter()
            .any(|chunk| chunk.skip_rewrite && chunk.text == "-"));

        let rebuilt = chunks
            .iter()
            .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
            .collect::<String>();
        assert_eq!(rebuilt, text);
    }

    #[test]
    fn marks_markdown_emphasis_markers_as_skip_rewrite() {
        let text = "前文 **很重要** 后文。";
        let chunks = segment_text(text, ChunkPreset::Sentence);
        assert!(chunks
            .iter()
            .any(|chunk| chunk.skip_rewrite && chunk.text.contains("**")));
        assert!(chunks
            .iter()
            .any(|chunk| !chunk.skip_rewrite && chunk.text.contains("很重要")));

        let rebuilt = chunks
            .iter()
            .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
            .collect::<String>();
        assert_eq!(rebuilt, text);
    }

    #[test]
    fn marks_markdown_footnote_references_as_skip_rewrite() {
        let text = "这是[^1]引用。";
        let chunks = segment_text(text, ChunkPreset::Sentence);
        assert!(chunks
            .iter()
            .any(|chunk| chunk.skip_rewrite && chunk.text.contains("[^1]")));

        let rebuilt = chunks
            .iter()
            .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
            .collect::<String>();
        assert_eq!(rebuilt, text);
    }

    #[test]
    fn marks_markdown_footnote_definition_prefix_as_skip_rewrite() {
        let text = "[^1]: 脚注内容。";
        let chunks = segment_text(text, ChunkPreset::Sentence);
        assert!(chunks
            .iter()
            .any(|chunk| chunk.skip_rewrite && chunk.text.contains("[^1]:")));
        assert!(chunks
            .iter()
            .any(|chunk| !chunk.skip_rewrite && chunk.text.contains("脚注内容")));

        let rebuilt = chunks
            .iter()
            .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
            .collect::<String>();
        assert_eq!(rebuilt, text);
    }

    #[test]
    fn marks_pandoc_citations_as_skip_rewrite() {
        let text = "如文献[@doe2020]所述。";
        let chunks = segment_text(text, ChunkPreset::Sentence);
        assert!(chunks
            .iter()
            .any(|chunk| { chunk.skip_rewrite && chunk.text.contains("[@doe2020]") }));

        let rebuilt = chunks
            .iter()
            .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
            .collect::<String>();
        assert_eq!(rebuilt, text);
    }

    #[test]
    fn marks_markdown_html_comments_as_skip_rewrite() {
        let text = "前文 <!-- 注释 --> 后文。";
        let chunks = segment_text(text, ChunkPreset::Sentence);
        assert!(chunks
            .iter()
            .any(|chunk| { chunk.skip_rewrite && chunk.text.contains("<!-- 注释 -->") }));

        let rebuilt = chunks
            .iter()
            .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
            .collect::<String>();
        assert_eq!(rebuilt, text);
    }

    #[test]
    fn ai_group_validation_requires_full_contiguous_coverage() {
        assert!(validate_ai_groups(&[vec![0, 1], vec![2]], 3).is_ok());
        assert!(validate_ai_groups(&[vec![0, 2]], 3).is_err());
        assert!(validate_ai_groups(&[vec![0], vec![1], vec![1]], 3).is_err());
        assert!(validate_ai_groups(&[], 2).is_err());
    }

    #[test]
    fn builds_chunks_from_ai_groups_preserving_original_text() {
        let text = "此外，本文提出一种方法，它在长句里也能保持连贯。";
        let atoms = vec![
            SegmentedChunk {
                text: "此外，".to_string(),
                separator_after: String::new(),
                skip_rewrite: false,
            },
            SegmentedChunk {
                text: "本文提出一种方法，".to_string(),
                separator_after: String::new(),
                skip_rewrite: false,
            },
            SegmentedChunk {
                text: "它在长句里也能保持连贯。".to_string(),
                separator_after: String::new(),
                skip_rewrite: false,
            },
        ];
        let groups = vec![vec![0, 1], vec![2]];
        let chunks = build_chunks_from_ai_groups(&atoms, &groups);

        let rebuilt = chunks
            .iter()
            .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
            .collect::<String>();
        assert_eq!(rebuilt, text);
    }

    #[test]
    fn detects_problematic_windows_for_ai_segmentation() {
        let text = format!(
            "{}{}{}{}",
            "这是一段没有明显句号但会持续变长的说明文字".repeat(24),
            "\n\n",
            "此外，".repeat(5),
            "最后补一句。"
        );
        let chunks = segment_text(&text, ChunkPreset::Sentence);
        let windows = collect_ai_segmentation_windows(&chunks, ChunkPreset::Sentence);
        assert!(!windows.is_empty());
    }

    #[test]
    fn detects_stream_required_api_errors() {
        let body = r#"{"error":{"message":"Stream must be set to true","type":"bad_response_status_code","param":"stream"}}"#;
        assert!(response_requires_stream(
            reqwest::StatusCode::BAD_REQUEST,
            body
        ));
        assert!(!response_requires_stream(
            reqwest::StatusCode::SERVICE_UNAVAILABLE,
            body
        ));
    }

    #[test]
    fn extracts_compact_api_error_message() {
        let body = r#"{"error":{"message":"Service temporarily unavailable","type":"api_error"}}"#;
        assert_eq!(
            extract_api_error_message(body).as_deref(),
            Some("Service temporarily unavailable")
        );
    }

    #[test]
    fn parses_sse_stream_chat_response_body() {
        let body = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"你\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"好\"}}]}\n\n",
            "data: [DONE]\n"
        );
        assert_eq!(
            parse_stream_chat_response_body(body).unwrap(),
            "你好".to_string()
        );
    }
}
