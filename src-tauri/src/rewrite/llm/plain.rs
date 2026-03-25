use crate::models::AppSettings;

use super::prompt::{
    merge_extra_constraints, resolve_system_prompt, EXTRA_CONSTRAINT_NO_MODEL_META,
    EXTRA_CONSTRAINT_NO_MODEL_META_RETRY,
};

use super::validate::validate_rewrite_output;

use super::super::text::{
    collapse_line_breaks_to_spaces, enforce_line_skeleton, normalize_line_endings_to_lf,
    split_line_skeleton, strip_redundant_prefix, trim_ascii_spaces_tabs_start,
};

async fn call_rewrite_model(
    client: &reqwest::Client,
    settings: &AppSettings,
    system_prompt: &str,
    user_prompt: &str,
    temperature: f32,
) -> Result<String, String> {
    super::transport::call_chat_model(client, settings, system_prompt, user_prompt, temperature)
        .await
}

async fn rewrite_plain_chunk_with_client_once(
    client: &reqwest::Client,
    settings: &AppSettings,
    system_prompt: &str,
    source_text: &str,
    extra_constraint: Option<&str>,
    temperature: f32,
) -> Result<String, String> {
    let multiline = source_text.contains('\n') || source_text.contains('\r');
    if multiline {
        let source_lines = split_lines_keep_empty(source_text);
        if source_lines.iter().all(|line| line.trim().is_empty()) {
            // 纯空白/纯缩进：不走模型，直接原样返回。
            return Ok(source_text.to_string());
        }

        let (user_prompt, expected_lines) =
            build_multiline_rewrite_prompt(source_text, extra_constraint);
        let sanitized =
            call_rewrite_model(client, settings, system_prompt, &user_prompt, temperature).await?;

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
                            system_prompt,
                            &source_lines,
                            extra_constraint,
                            temperature,
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
                            system_prompt,
                            &source_lines,
                            extra_constraint,
                            temperature,
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

        let user_prompt = build_singleline_rewrite_prompt(&core, extra_constraint);
        let sanitized =
            call_rewrite_model(client, settings, system_prompt, &user_prompt, temperature).await?;
        let rewritten = collapse_line_breaks_to_spaces(&sanitized);

        let body = strip_redundant_prefix(&rewritten, &prefix);
        if body.trim().is_empty() {
            return Ok(source_text.to_string());
        }

        Ok(format!("{prefix}{body}{suffix}"))
    }
}

pub(super) async fn rewrite_plain_chunk_with_client(
    client: &reqwest::Client,
    settings: &AppSettings,
    source_text: &str,
    extra_constraint: Option<&str>,
) -> Result<String, String> {
    super::validate_settings(settings)?;

    let system_prompt = resolve_system_prompt(settings);
    let base_constraint =
        merge_extra_constraints(extra_constraint, &[EXTRA_CONSTRAINT_NO_MODEL_META]);
    let retry_constraint = merge_extra_constraints(
        base_constraint.as_deref(),
        &[EXTRA_CONSTRAINT_NO_MODEL_META_RETRY],
    );

    let mut last_error: Option<String> = None;

    for (attempt, temperature, constraint) in [
        (1usize, settings.temperature, base_constraint.as_deref()),
        (2usize, 0.0, retry_constraint.as_deref()),
    ] {
        let result = rewrite_plain_chunk_with_client_once(
            client,
            settings,
            &system_prompt,
            source_text,
            constraint,
            temperature,
        )
        .await;

        match result {
            Ok(candidate) => match validate_rewrite_output(source_text, &candidate) {
                Ok(()) => return Ok(candidate),
                Err(error) => {
                    last_error = Some(error);
                    if attempt >= 2 {
                        break;
                    }
                }
            },
            Err(error) => {
                last_error = Some(error);
                if attempt >= 2 {
                    break;
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| "模型改写失败。".to_string()))
}

async fn rewrite_single_line_with_client(
    client: &reqwest::Client,
    settings: &AppSettings,
    system_prompt: &str,
    source_text: &str,
    extra_constraint: Option<&str>,
    temperature: f32,
) -> Result<String, String> {
    let (prefix, core, suffix) = split_line_skeleton(source_text);
    if core.trim().is_empty() {
        return Ok(source_text.to_string());
    }

    let user_prompt = build_singleline_rewrite_prompt(&core, extra_constraint);
    let sanitized =
        call_rewrite_model(client, settings, system_prompt, &user_prompt, temperature).await?;
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
    extra_constraint: Option<&str>,
    temperature: f32,
) -> Result<String, String> {
    let mut rebuilt = Vec::with_capacity(source_lines.len());
    for line in source_lines.iter() {
        if line.trim().is_empty() {
            // 空行/仅空白的行属于格式骨架：原样保留（包括可能存在的空格缩进）。
            rebuilt.push(line.to_string());
            continue;
        }

        let rewritten_line = rewrite_single_line_with_client(
            client,
            settings,
            system_prompt,
            line,
            extra_constraint,
            temperature,
        )
        .await?;
        rebuilt.push(rewritten_line);
    }

    Ok(rebuilt.join("\n"))
}

fn build_singleline_rewrite_prompt(source_body: &str, extra_constraint: Option<&str>) -> String {
    let extra_constraint = extra_constraint
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(|value| format!("\n\n额外约束（必须遵守）：\n- {value}"))
        .unwrap_or_default();
    format!(
        "请改写下面这段文字。保留原意与信息密度，尽量减少机械重复感。不要添加解释。\n\n格式要求（必须遵守）：\n- 严格保持原文的换行/空行/缩进/列表符号与标点风格，不要新增或删除换行。\n- 不要输出 Markdown（尤其不要使用行尾两个空格来制造换行）。\n- 只输出改写后的正文，不要输出任何标签或解释。{extra_constraint}\n\n原文：\n{source_body}"
    )
}

fn build_multiline_rewrite_prompt(
    source_body: &str,
    extra_constraint: Option<&str>,
) -> (String, usize) {
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

    let extra_constraint = extra_constraint
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(|value| format!("\n- {value}"))
        .unwrap_or_default();

    let prompt = format!(
        "请对下面的文本进行改写，让表达更自然，但必须【严格保持行结构不变】。\n\n输入格式：每行以 @@@序号@@@ 开头（序号从 1 开始）。\n输出要求（必须遵守）：\n- 必须输出【相同数量】的行，行序号必须从 1 到 {expected} 连续且不重复。\n- 每行必须保留对应的 @@@序号@@@ 前缀，且不得新增、删除、合并或拆分任何一行。\n- 每行改写后的内容必须在同一行内，不得包含换行符。\n- 空行（只有前缀没有内容）必须原样输出为空行（仍保留前缀）。\n- 不要输出 Markdown/代码块/解释/标题，只输出这些行。{extra_constraint}\n\n原文：\n{template}"
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
