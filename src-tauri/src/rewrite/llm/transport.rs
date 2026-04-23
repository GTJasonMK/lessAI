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
    let response =
        send_chat_request(client, settings, system_prompt, user_prompt, temperature).await?;

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

    if !status.is_success() {
        return Err(format_chat_api_error(status, &body));
    }

    parse_chat_response_body(&content_type, &body)
}

fn parse_chat_response_body(content_type: &str, body: &str) -> Result<String, String> {
    if content_type.contains("text/event-stream") || content_type.contains("ndjson") {
        return parse_stream_chat_response_body(body);
    }

    parse_json_chat_response_body(body)
}

async fn send_chat_request(
    client: &reqwest::Client,
    settings: &AppSettings,
    system_prompt: &str,
    user_prompt: &str,
    temperature: f32,
) -> Result<reqwest::Response, String> {
    let request_body = json!({
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

    client
        .post(chat_url(&settings.base_url))
        .header(AUTHORIZATION, format!("Bearer {}", settings.api_key))
        .header(CONTENT_TYPE, "application/json")
        .header(
            ACCEPT,
            "application/json, text/event-stream, application/x-ndjson",
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
    sanitize_completion_text(extract_response_content(&value, ResponseContentMode::Json))
}

pub(in crate::rewrite) fn parse_stream_chat_response_body(body: &str) -> Result<String, String> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return Err("模型返回内容为空。".to_string());
    }

    if body_looks_like_ndjson(body) {
        return parse_ndjson_chat_response_body(body);
    }

    parse_sse_chat_response_body(body)
}

fn parse_ndjson_chat_response_body(body: &str) -> Result<String, String> {
    let mut merged = String::new();
    let mut saw_json = false;

    for raw_line in body.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if !line.starts_with('{') {
            return Err("流式响应解析失败：NDJSON 行不是 JSON 对象。".to_string());
        }

        let value: Value =
            serde_json::from_str(line).map_err(|error| format!("流式响应解析失败：{error}"))?;
        saw_json = true;
        if let Some(delta) = extract_response_content(&value, ResponseContentMode::Stream) {
            merged.push_str(&delta);
        }
    }

    if !saw_json {
        return Err("流式响应解析失败：无法识别 NDJSON 内容。".to_string());
    }

    sanitize_completion_text(Some(merged))
}

fn parse_sse_chat_response_body(body: &str) -> Result<String, String> {
    let mut merged = String::new();
    let mut saw_data = false;

    for raw_line in body.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with(':') || line.starts_with("event:") {
            continue;
        }
        if !line.starts_with("data:") {
            return Err("流式响应解析失败：缺少 data 行。".to_string());
        }

        saw_data = true;
        let payload = line["data:".len()..].trim();
        if payload.is_empty() || payload == "[DONE]" {
            continue;
        }

        let value: Value =
            serde_json::from_str(payload).map_err(|error| format!("流式响应解析失败：{error}"))?;
        if let Some(delta) = extract_response_content(&value, ResponseContentMode::Stream) {
            merged.push_str(&delta);
        }
    }

    if !saw_data {
        return Err("流式响应解析失败：缺少 data 行。".to_string());
    }

    sanitize_completion_text(Some(merged))
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

    let sanitized = content.trim().to_string();
    if sanitized.is_empty() {
        return Err("模型返回内容为空。".to_string());
    }

    Ok(sanitized)
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

#[derive(Clone, Copy)]
enum ResponseContentMode {
    Json,
    Stream,
}

fn extract_response_content(value: &Value, mode: ResponseContentMode) -> Option<String> {
    let paths = match mode {
        ResponseContentMode::Json => &[
            ChoiceTextPath::MessageContent,
            ChoiceTextPath::DeltaContent,
            ChoiceTextPath::Text,
        ][..],
        ResponseContentMode::Stream => &[
            ChoiceTextPath::DeltaContent,
            ChoiceTextPath::DeltaText,
            ChoiceTextPath::MessageContent,
            ChoiceTextPath::Text,
        ][..],
    };

    extract_choice_text(
        &value["choices"][0],
        paths,
    )
}

#[derive(Clone, Copy)]
enum ChoiceTextPath {
    MessageContent,
    DeltaContent,
    DeltaText,
    Text,
}

fn extract_choice_text(choice: &Value, paths: &[ChoiceTextPath]) -> Option<String> {
    for path in paths {
        let value = match path {
            ChoiceTextPath::MessageContent => &choice["message"]["content"],
            ChoiceTextPath::DeltaContent => &choice["delta"]["content"],
            ChoiceTextPath::DeltaText => &choice["delta"]["text"],
            ChoiceTextPath::Text => &choice["text"],
        };
        if let Some(text) = extract_text_field(value) {
            return Some(text);
        }
    }
    None
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
