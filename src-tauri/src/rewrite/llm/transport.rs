use std::error::Error as _;

use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use serde_json::{json, Value};

use crate::models::AppSettings;

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

pub(super) async fn call_chat_model(
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
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    let body = response
        .text()
        .await
        .map_err(|error| format_reqwest_error(error))?;

    if status.is_success() {
        let prefer_stream_parser = content_type.contains("text/event-stream")
            || content_type.contains("ndjson")
            || body_looks_like_sse(&body);

        if prefer_stream_parser {
            return parse_stream_chat_response_body(&body);
        }

        return match parse_json_chat_response_body(&body) {
            Ok(text) => Ok(text),
            Err(error) => {
                // 兼容“强制流式输出但没有正确标 Content-Type”的上游：
                // - 可能返回 NDJSON（多行 JSON）
                // - 也可能直接返回 SSE 文本但首行不是 `data:`（极少见）
                if body_looks_like_ndjson(&body) {
                    parse_stream_chat_response_body(&body)
                } else {
                    Err(error)
                }
            }
        };
    }

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
        .header(
            ACCEPT,
            if stream {
                "text/event-stream"
            } else {
                "application/json"
            },
        )
        .json(&request_body)
        .send()
        .await
        .map_err(format_reqwest_error)
}

fn parse_json_chat_response_body(body: &str) -> Result<String, String> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return Err("模型返回内容为空。".to_string());
    }
    let value: Value =
        serde_json::from_str(trimmed).map_err(|error| format!("响应解析失败：{error}"))?;
    sanitize_completion_text(extract_content(&value))
}

pub(in crate::rewrite) fn parse_stream_chat_response_body(body: &str) -> Result<String, String> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return Err("模型返回内容为空。".to_string());
    }

    if trimmed.starts_with('{') {
        if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
            return sanitize_completion_text(extract_content(&value));
        }

        // 兼容：上游返回“纯文本”但正文恰好以 `{` 开头（例如代码/配置片段）。
        // 此时不应强行按 JSON/NDJSON 解析，否则会把合法文本误判为错误。
        if !body_looks_like_ndjson(body) {
            return sanitize_completion_text(Some(trimmed.to_string()));
        }

        // 兼容少数上游用 NDJSON 进行流式输出：每行一个 JSON 对象。
        let mut merged = String::new();
        let mut saw_json = false;
        for raw_line in body.lines() {
            let line = raw_line.trim();
            if line.is_empty() {
                continue;
            }
            if !line.starts_with('{') {
                continue;
            }
            let value: Value =
                serde_json::from_str(line).map_err(|error| format!("流式响应解析失败：{error}"))?;
            saw_json = true;
            if let Some(delta) = extract_stream_content(&value).or_else(|| extract_content(&value))
            {
                merged.push_str(&delta);
            }
        }
        if saw_json && !merged.is_empty() {
            return sanitize_completion_text(Some(merged));
        }
        return Err("流式响应解析失败：无法识别 NDJSON 内容。".to_string());
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

pub(in crate::rewrite) fn response_requires_stream(
    status: reqwest::StatusCode,
    body: &str,
) -> bool {
    if status != reqwest::StatusCode::BAD_REQUEST
        && status != reqwest::StatusCode::UNPROCESSABLE_ENTITY
    {
        return false;
    }

    let normalized = body.to_ascii_lowercase();
    // 最明确的固定模板（兼容已有测试）。
    normalized.contains("stream must be set to true")
        || (normalized.contains("\"param\":\"stream\"")
            && normalized.contains("must be set to true"))
        || (normalized.contains("\"stream\"") && normalized.contains("set to true"))
        // 更宽松的兜底：上游表述不同，但明确要求 stream=true。
        || ((normalized.contains("stream") && normalized.contains("true"))
            && (normalized.contains("must")
                || normalized.contains("required")
                || normalized.contains("only support")
                || normalized.contains("only supports")
                || normalized.contains("set to")))
        // 中文表述：`stream 必须为 true` / `stream=true`
        || (normalized.contains("stream") && normalized.contains("true") && body.contains("必须"))
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

pub(in crate::rewrite) fn extract_api_error_message(body: &str) -> Option<String> {
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

fn body_looks_like_sse(body: &str) -> bool {
    // SSE 通常以 `data:`（或 `event:`/`:` 注释）为行前缀。
    for raw_line in body.lines() {
        let line = raw_line.trim_start();
        if line.is_empty() {
            continue;
        }
        return line.starts_with("data:") || line.starts_with("event:") || line.starts_with(':');
    }
    false
}

fn body_looks_like_ndjson(body: &str) -> bool {
    let mut json_lines = 0usize;
    for raw_line in body.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        // SSE 不应走 NDJSON 判断。
        if line.starts_with("data:") || line.starts_with("event:") || line.starts_with(':') {
            return false;
        }
        if line.starts_with('{') && line.ends_with('}') {
            json_lines = json_lines.saturating_add(1);
            if json_lines >= 2 {
                return true;
            }
            continue;
        }
        return false;
    }
    false
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
