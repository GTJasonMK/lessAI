use crate::models::AppSettings;

pub(crate) const MAX_CONCURRENCY_LIMIT: usize = 8;
const MIN_TIMEOUT_MS: u64 = 1_000;

pub(crate) fn validate_numeric_settings(settings: &AppSettings) -> Result<(), String> {
    if settings.timeout_ms < MIN_TIMEOUT_MS {
        return Err(format!("超时时间必须大于等于 {MIN_TIMEOUT_MS} 毫秒。"));
    }
    if !(0.0..=2.0).contains(&settings.temperature) {
        return Err("温度必须在 0 到 2 之间。".to_string());
    }
    if !(1..=MAX_CONCURRENCY_LIMIT).contains(&settings.max_concurrency) {
        return Err(format!(
            "自动并发数必须在 1 到 {MAX_CONCURRENCY_LIMIT} 之间。"
        ));
    }
    if settings.units_per_batch == 0 {
        return Err("单批处理单元数必须大于等于 1。".to_string());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{validate_numeric_settings, MAX_CONCURRENCY_LIMIT};
    use crate::models::AppSettings;

    #[test]
    fn rejects_timeout_below_minimum() {
        let mut settings = AppSettings::default();
        settings.timeout_ms = 999;

        let error = validate_numeric_settings(&settings).expect_err("expected invalid timeout");

        assert_eq!(error, "超时时间必须大于等于 1000 毫秒。");
    }

    #[test]
    fn rejects_temperature_out_of_range() {
        let mut settings = AppSettings::default();
        settings.temperature = 2.1;

        let error = validate_numeric_settings(&settings).expect_err("expected invalid temperature");

        assert_eq!(error, "温度必须在 0 到 2 之间。");
    }

    #[test]
    fn rejects_zero_units_per_batch() {
        let mut settings = AppSettings::default();
        settings.units_per_batch = 0;

        let error = validate_numeric_settings(&settings).expect_err("expected invalid batch size");

        assert_eq!(error, "单批处理单元数必须大于等于 1。");
    }

    #[test]
    fn rejects_max_concurrency_above_limit() {
        let mut settings = AppSettings::default();
        settings.max_concurrency = MAX_CONCURRENCY_LIMIT + 1;

        let error = validate_numeric_settings(&settings).expect_err("expected invalid concurrency");

        assert_eq!(error, "自动并发数必须在 1 到 8 之间。");
    }
}
