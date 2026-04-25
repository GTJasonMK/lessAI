pub fn normalize_proxy_url(raw_proxy: &str) -> Option<String> {
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

#[cfg(test)]
mod tests {
    use super::normalize_proxy_url;

    #[test]
    fn normalize_proxy_url_returns_none_for_empty() {
        assert_eq!(normalize_proxy_url("   "), None);
    }

    #[test]
    fn normalize_proxy_url_adds_http_scheme_for_host_port() {
        assert_eq!(
            normalize_proxy_url("127.0.0.1:7890").as_deref(),
            Some("http://127.0.0.1:7890")
        );
    }

    #[test]
    fn normalize_proxy_url_keeps_existing_scheme() {
        assert_eq!(
            normalize_proxy_url("socks5h://127.0.0.1:7891").as_deref(),
            Some("socks5h://127.0.0.1:7891")
        );
    }
}
