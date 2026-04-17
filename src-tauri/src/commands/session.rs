use std::fs;

use chrono::Utc;
use tauri::{AppHandle, State};

use crate::{
    documents::document_session_id,
    models::DocumentSession,
    session_access::{access_current_session, CurrentSessionRequest},
    session_access::open_session_for_path,
    session_loader::load_clean_session_from_existing,
    state::{with_session_lock, AppState},
    storage,
};

#[tauri::command]
pub fn load_session(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
) -> Result<DocumentSession, String> {
    access_current_session(
        CurrentSessionRequest::refreshed(&app, state.inner(), &session_id),
        Ok,
    )
}

#[tauri::command]
pub fn reset_session(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
) -> Result<DocumentSession, String> {
    access_current_session(
        CurrentSessionRequest::stored(&app, state.inner(), &session_id)
            .with_active_job_error("当前文档正在执行自动任务，请先暂停并取消后再重置。"),
        |existing| {
            // 重置是“清空会话记录并重建切块”，不修改原文件。
            let session = load_clean_session_from_existing(&app, &existing, Utc::now(), true)?;
            storage::save_session(&app, &session)?;
            Ok(session)
        },
    )
}

#[tauri::command]
pub fn open_document(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<DocumentSession, String> {
    if path.trim().is_empty() {
        return Err("文件路径不能为空。".to_string());
    }

    let canonical = fs::canonicalize(&path)
        .map_err(|error| format!("无法打开文件（路径无效或文件不存在）：{error}"))?;
    let meta = fs::metadata(&canonical)
        .map_err(|error| format!("无法读取文件信息（可能无权限或文件不存在）：{error}"))?;
    if !meta.is_file() {
        return Err("所选路径不是文件，请选择一个文档文件。".to_string());
    }
    let canonical_str = canonical.to_string_lossy().to_string();
    let session_id = document_session_id(&canonical_str);

    with_session_lock(state.inner(), &session_id, || {
        open_session_for_path(&app, &session_id, &canonical, canonical_str, Utc::now())
    })
}
