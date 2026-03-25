use std::{fs, path::PathBuf};

use tauri::{AppHandle, State};

use crate::{
    documents::ensure_document_can_write_back,
    models::RunningState,
    rewrite, rewrite_jobs,
    state::{with_session_lock, AppState},
    storage,
};

#[tauri::command]
pub fn export_document(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> Result<String, String> {
    let session = with_session_lock(state.inner(), &session_id, || {
        storage::load_session(&app, &session_id)
    })?;
    let line_ending = rewrite::detect_line_ending(&session.source_text);
    let mut content = rewrite_jobs::build_merged_text(&session);
    if !rewrite::has_trailing_spaces_per_line(&session.source_text) {
        content = rewrite::strip_trailing_spaces_per_line(&content);
    }
    content = rewrite::convert_line_endings(&content, line_ending);
    let path_buf = PathBuf::from(&path);

    if let Some(parent) = path_buf.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    fs::write(&path_buf, content).map_err(|error| error.to_string())?;
    Ok(path)
}

#[tauri::command]
pub fn finalize_document(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
) -> Result<String, String> {
    {
        // 避免与后台 job 竞争写 session 文件/源文件；如果任务仍在运行或退出中，直接拒绝。
        let jobs = state
            .jobs
            .lock()
            .map_err(|_| "任务状态锁已损坏。".to_string())?;
        if jobs.contains_key(&session_id) {
            return Err("后台任务仍在运行或正在退出，请稍后再试。".to_string());
        }
    }

    with_session_lock(state.inner(), &session_id, || {
        let session = storage::load_session(&app, &session_id)?;

        if matches!(session.status, RunningState::Running | RunningState::Paused) {
            return Err("当前文档正在执行自动任务，请先暂停并取消后再写回原文件。".to_string());
        }

        ensure_document_can_write_back(&session.document_path)?;

        let line_ending = rewrite::detect_line_ending(&session.source_text);
        let mut content = rewrite_jobs::build_merged_text(&session);
        if !rewrite::has_trailing_spaces_per_line(&session.source_text) {
            content = rewrite::strip_trailing_spaces_per_line(&content);
        }
        content = rewrite::convert_line_endings(&content, line_ending);
        let target = PathBuf::from(&session.document_path);

        // 保险起见：确保父目录存在（大多数情况下本来就存在）。
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }

        // 覆盖写回原文件：只写入“已应用”的修改，未应用的候选不会进入文件。
        fs::write(&target, content).map_err(|error| error.to_string())?;

        // 写回成功后再清理记录，避免“写失败但记录被删”的风险。
        storage::delete_session(&app, &session_id)?;

        Ok(session.document_path)
    })
}
