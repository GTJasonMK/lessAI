use tauri::AppHandle;

use crate::{models::AppSettings, rewrite, storage};

#[tauri::command]
pub fn load_settings(app: AppHandle) -> Result<AppSettings, String> {
    storage::load_settings(&app)
}

#[tauri::command]
pub fn save_settings(app: AppHandle, settings: AppSettings) -> Result<AppSettings, String> {
    storage::save_settings(&app, &settings)
}

#[tauri::command]
pub async fn test_provider(
    settings: AppSettings,
) -> Result<crate::models::ProviderCheckResult, String> {
    rewrite::test_provider(&settings).await
}
