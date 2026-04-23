use std::time::Duration;

use crate::models::{AppSettings, DocumentFormat, ProviderCheckResult};
use crate::rewrite_unit::{
    parse_rewrite_batch_response, parse_rewrite_unit_response, RewriteBatchRequest,
    RewriteBatchResponse, RewriteUnitRequest, RewriteUnitResponse,
};
use crate::settings_validation::validate_numeric_settings;

mod plain_support;
mod selection;
pub(in crate::rewrite) mod transport;
mod validate;

pub fn build_client(settings: &AppSettings) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder().timeout(Duration::from_millis(settings.timeout_ms));

    if let Some(proxy_url) = normalize_proxy_url(&settings.update_proxy) {
        let proxy = reqwest::Proxy::all(&proxy_url)
            .map_err(|error| format!("代理地址无效（{proxy_url}）：{error}"))?;
        builder = builder.no_proxy().proxy(proxy);
    }

    builder.build().map_err(|error| error.to_string())
}

fn normalize_proxy_url(raw_proxy: &str) -> Option<String> {
    let trimmed = raw_proxy.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.contains("://") {
        Some(trimmed.to_string())
    } else {
        Some(format!("http://{trimmed}"))
    }
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

pub async fn rewrite_selection_text_with_client(
    client: &reqwest::Client,
    settings: &AppSettings,
    source_text: &str,
    format: DocumentFormat,
    rewrite_headings: bool,
) -> Result<String, String> {
    selection::rewrite_selection_text_with_client(
        client,
        settings,
        source_text,
        format,
        rewrite_headings,
    )
    .await
}

pub async fn rewrite_unit_with_client(
    client: &reqwest::Client,
    settings: &AppSettings,
    request: &RewriteUnitRequest,
) -> Result<RewriteUnitResponse, String> {
    let system_prompt = request.system_prompt();
    let user_prompt = request.user_prompt();
    let raw = transport::call_chat_model(
        client,
        settings,
        &system_prompt,
        &user_prompt,
        settings.temperature,
    )
    .await?;
    parse_rewrite_unit_response(request, &raw)
}

pub async fn rewrite_batch_with_client(
    client: &reqwest::Client,
    settings: &AppSettings,
    request: &RewriteBatchRequest,
) -> Result<RewriteBatchResponse, String> {
    let system_prompt = request.system_prompt();
    let user_prompt = request.user_prompt();
    let raw = transport::call_chat_model(
        client,
        settings,
        &system_prompt,
        &user_prompt,
        settings.temperature,
    )
    .await?;
    parse_rewrite_batch_response(request, &raw)
}

pub async fn rewrite_selection_text(
    settings: &AppSettings,
    source_text: &str,
    format: DocumentFormat,
    rewrite_headings: bool,
) -> Result<String, String> {
    let client = build_client(settings)?;
    rewrite_selection_text_with_client(&client, settings, source_text, format, rewrite_headings)
        .await
}

pub async fn rewrite_batch(
    settings: &AppSettings,
    request: &RewriteBatchRequest,
) -> Result<RewriteBatchResponse, String> {
    let client = build_client(settings)?;
    rewrite_batch_with_client(&client, settings, request).await
}

fn validate_settings(settings: &AppSettings) -> Result<(), String> {
    validate_numeric_settings(settings)?;
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

#[cfg(test)]
mod tests {
    use super::{build_client, normalize_proxy_url, validate_settings};
    use crate::models::AppSettings;

    #[test]
    fn validate_settings_rejects_zero_units_per_batch() {
        let mut settings = valid_settings();
        settings.units_per_batch = 0;

        let error = validate_settings(&settings).expect_err("expected invalid batch size");

        assert_eq!(error, "单批处理单元数必须大于等于 1。");
    }

    #[test]
    fn validate_settings_rejects_max_concurrency_above_limit() {
        let mut settings = valid_settings();
        settings.max_concurrency = 9;

        let error = validate_settings(&settings).expect_err("expected invalid max concurrency");

        assert_eq!(error, "自动并发数必须在 1 到 8 之间。");
    }

    #[test]
    fn normalize_proxy_url_adds_http_scheme_for_host_port() {
        let normalized = normalize_proxy_url("127.0.0.1:7890");

        assert_eq!(normalized.as_deref(), Some("http://127.0.0.1:7890"));
    }

    #[test]
    fn normalize_proxy_url_keeps_existing_scheme() {
        let normalized = normalize_proxy_url("socks5h://127.0.0.1:7891");

        assert_eq!(normalized.as_deref(), Some("socks5h://127.0.0.1:7891"));
    }

    #[test]
    fn build_client_rejects_invalid_proxy() {
        let mut settings = valid_settings();
        settings.update_proxy = "://bad".to_string();

        let error = build_client(&settings).expect_err("expected invalid proxy url");

        assert!(error.contains("代理地址无效"), "unexpected error: {error}");
    }

    fn valid_settings() -> AppSettings {
        AppSettings {
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: "test-key".to_string(),
            model: "gpt-4.1-mini".to_string(),
            ..AppSettings::default()
        }
    }
}
