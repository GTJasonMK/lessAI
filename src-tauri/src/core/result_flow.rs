pub(crate) fn load_then<T, Loaded, Load, Apply>(load: Load, apply: Apply) -> Result<T, String>
where
    Load: FnOnce() -> Result<Loaded, String>,
    Apply: FnOnce(Loaded) -> Result<T, String>,
{
    let loaded = load()?;
    apply(loaded)
}

#[cfg(test)]
mod tests {
    #[test]
    fn load_then_runs_apply_after_successful_load() {
        let value = super::load_then(|| Ok("loaded"), |loaded: &str| Ok(loaded.len()))
            .expect("expected load_then to apply after load");

        assert_eq!(value, 6);
    }

    #[test]
    fn load_then_returns_load_error_before_apply() {
        let mut applied = false;

        let error = super::load_then(
            || Err::<&'static str, String>("load failed".to_string()),
            |_: &'static str| {
                applied = true;
                Ok(())
            },
        )
        .expect_err("expected load_then to return load error");

        assert_eq!(error, "load failed");
        assert!(!applied);
    }
}
