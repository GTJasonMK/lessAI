use std::time::Duration;

use crate::models::{AppSettings, DocumentFormat, ProviderCheckResult};

mod markdown;
mod plain;
mod prompt;
mod tex;
pub(in crate::rewrite) mod transport;
mod validate;

pub fn build_client(settings: &AppSettings) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_millis(settings.timeout_ms))
        .build()
        .map_err(|error| error.to_string())
}

pub async fn test_provider(settings: &AppSettings) -> Result<ProviderCheckResult, String> {
    validate_settings(settings)?;

    let client = build_client(settings)?;
    let probe =
        transport::call_chat_model(&client, settings, "你是连通性探针。只回复 OK。", "OK", 0.0)
            .await;

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
    format: DocumentFormat,
) -> Result<String, String> {
    match format {
        DocumentFormat::Tex => {
            tex::rewrite_tex_chunk_with_client(client, settings, source_text).await
        }
        DocumentFormat::Markdown => {
            markdown::rewrite_markdown_chunk_with_client(client, settings, source_text).await
        }
        DocumentFormat::PlainText => {
            plain::rewrite_plain_chunk_with_client(client, settings, source_text, None).await
        }
    }
}

pub async fn rewrite_chunk(
    settings: &AppSettings,
    source_text: &str,
    format: DocumentFormat,
) -> Result<String, String> {
    let client = build_client(settings)?;
    rewrite_chunk_with_client(&client, settings, source_text, format).await
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
