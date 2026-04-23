use std::{
    fs,
    path::{Path, PathBuf},
};

use tauri::{AppHandle, Manager};

use crate::{
    documents::hydrated_session_clone,
    atomic_write::write_bytes_atomically,
    models::{AppSettings, DocumentSession},
    settings_validation::validate_numeric_settings,
};

const SETTINGS_FILE: &str = "settings.json";
const SESSIONS_DIR: &str = "sessions";

fn app_root(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    Ok(dir)
}

fn sessions_root(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app_root(app)?.join(SESSIONS_DIR);
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    Ok(dir)
}

fn session_path(app: &AppHandle, session_id: &str) -> Result<PathBuf, String> {
    Ok(sessions_root(app)?.join(format!("{session_id}.json")))
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app_root(app)?.join(SETTINGS_FILE))
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let content = fs::read_to_string(path).map_err(|error| error.to_string())?;
    serde_json::from_str(&content).map_err(|error| error.to_string())
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let content = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    write_json_bytes(path, &content)
}

fn write_json_bytes(path: &Path, payload: &[u8]) -> Result<(), String> {
    write_bytes_atomically(path, payload)?;
    restrict_json_file_permissions(path);
    Ok(())
}

fn restrict_json_file_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // settings/session 可能包含敏感信息（例如 API Key、草稿内容），尽量限制文件权限。
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

pub fn load_settings(app: &AppHandle) -> Result<AppSettings, String> {
    let path = settings_path(app)?;
    match load_settings_from_path(&path) {
        Ok(settings) => Ok(settings),
        Err(error) => {
            log::error!(
                "load settings failed: path={} error={error}",
                path.display()
            );
            Err(error)
        }
    }
}

fn load_settings_from_path(path: &Path) -> Result<AppSettings, String> {
    if !path.exists() {
        return Ok(AppSettings::default());
    }

    let settings = read_json(path)
        .map_err(|error| format!("读取配置文件失败（{}）：{error}", path.display()))?;
    validate_numeric_settings(&settings)
        .map_err(|error| format!("配置文件无效（{}）：{error}", path.display()))?;
    Ok(settings)
}

pub fn save_settings(app: &AppHandle, settings: &AppSettings) -> Result<AppSettings, String> {
    let path = settings_path(app)?;
    validate_numeric_settings(settings)?;
    write_json(&path, settings)?;
    load_settings(app)
}

pub fn save_session(app: &AppHandle, session: &DocumentSession) -> Result<(), String> {
    let path = session_path(app, &session.id)?;
    write_json(&path, &hydrated_session_clone(session))
}

pub fn load_session(app: &AppHandle, session_id: &str) -> Result<DocumentSession, String> {
    let path = session_path(app, session_id)?;
    if !path.exists() {
        return Err(format!("未找到会话：{session_id}"));
    }

    let mut session = read_json(&path)?;
    crate::documents::hydrate_session_capabilities(&mut session);
    Ok(session)
}

pub fn load_session_optional(
    app: &AppHandle,
    session_id: &str,
) -> Result<Option<DocumentSession>, String> {
    let path = session_path(app, session_id)?;
    if !path.exists() {
        return Ok(None);
    }

    let mut session = read_json(&path)?;
    crate::documents::hydrate_session_capabilities(&mut session);
    Ok(Some(session))
}

pub fn delete_session(app: &AppHandle, session_id: &str) -> Result<(), String> {
    let path = session_path(app, session_id)?;
    if !path.exists() {
        return Ok(());
    }

    fs::remove_file(&path).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::validate_numeric_settings;
    use crate::{
        models::AppSettings,
        test_support::{cleanup_dir, unique_test_dir},
    };

    #[test]
    fn write_json_bytes_creates_parent_dirs_and_writes_payload() {
        let root = unique_test_dir("json-bytes-create");
        let target = root.join("nested").join("data.json");

        super::write_json_bytes(&target, br#"{"key":"value"}"#)
            .expect("expected json bytes helper to write payload");

        let stored = fs::read(&target).expect("read written json");
        assert_eq!(stored, br#"{"key":"value"}"#);
        cleanup_dir(&root);
    }

    #[test]
    fn write_json_bytes_replaces_existing_payload() {
        let root = unique_test_dir("json-bytes-replace");
        fs::create_dir_all(&root).expect("create root");
        let target = root.join("data.json");
        fs::write(&target, br#"{"old":true}"#).expect("seed old json");

        super::write_json_bytes(&target, br#"{"new":true}"#)
            .expect("expected json bytes helper to replace payload");

        let stored = fs::read(&target).expect("read replaced json");
        assert_eq!(stored, br#"{"new":true}"#);
        cleanup_dir(&root);
    }

    #[test]
    fn validate_numeric_settings_rejects_zero_units_per_batch() {
        let mut settings = AppSettings::default();
        settings.units_per_batch = 0;

        let error = validate_numeric_settings(&settings).expect_err("expected invalid batch size");

        assert_eq!(error, "单批处理单元数必须大于等于 1。");
    }

    #[test]
    fn load_settings_from_path_reports_path_context_for_missing_required_field() {
        let root = unique_test_dir("load-settings-missing-field");
        fs::create_dir_all(&root).expect("create root");
        let path = root.join("settings.json");
        fs::write(
            &path,
            r#"{
  "baseUrl": "https://api.openai.com/v1",
  "apiKey": "",
  "model": "gpt-4.1-mini",
  "updateProxy": "",
  "timeoutMs": 45000,
  "temperature": 0.8,
  "segmentationPreset": "small",
  "rewriteHeadings": false,
  "rewriteMode": "manual",
  "maxConcurrency": 2,
  "unitsPerBatch": 1,
  "promptPresetId": "humanizer_zh",
  "customPrompts": []
}"#,
        )
        .expect("write invalid settings");

        let error = super::load_settings_from_path(&path)
            .expect_err("expected invalid segmentation preset to fail");

        assert!(error.contains("settings.json"));
        assert!(error.contains("small"));
        cleanup_dir(&root);
    }

    #[test]
    fn load_settings_from_path_uses_defaults_for_missing_fields_without_rewriting_file() {
        let root = unique_test_dir("load-settings-default-missing-fields");
        fs::create_dir_all(&root).expect("create root");
        let path = root.join("settings.json");
        let original = r#"{
  "baseUrl": "https://api.openai.com/v1",
  "apiKey": "key",
  "model": "gpt-4.1-mini",
  "timeoutMs": 45000,
  "temperature": 0.8,
  "rewriteMode": "manual"
}"#;
        fs::write(&path, original).expect("write partial settings");

        let settings =
            super::load_settings_from_path(&path).expect("expected partial settings to load");

        assert_eq!(settings.api_key, "key");
        assert_eq!(
            settings.segmentation_preset,
            crate::models::SegmentationPreset::Paragraph
        );
        assert!(!settings.rewrite_headings);
        assert_eq!(settings.max_concurrency, 2);
        assert_eq!(settings.units_per_batch, 1);

        let stored = fs::read_to_string(&path).expect("read migrated settings");
        assert_eq!(stored, original);
        cleanup_dir(&root);
    }
}
