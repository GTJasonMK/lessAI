use std::{fs, path::PathBuf};

use tauri::{AppHandle, State};

use crate::{
    adapters::docx::DocxAdapter,
    atomic_write::write_bytes_atomically,
    documents::{
        ensure_document_can_write_back, is_docx_path, load_verified_writeback_bytes,
        write_document_content,
    },
    models::RunningState,
    rewrite, rewrite_jobs,
    session_repair::{repair_session_snapshot_if_needed, SnapshotRepairOutcome},
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

    write_bytes_atomically(&path_buf, content.as_bytes())?;
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
        let mut session = storage::load_session(&app, &session_id)?;
        if repair_session_snapshot_if_needed(&app, &mut session)? != SnapshotRepairOutcome::None {
            storage::save_session(&app, &session)?;
        }

        if matches!(session.status, RunningState::Running | RunningState::Paused) {
            return Err("当前文档正在执行自动任务，请先暂停并取消后再写回原文件。".to_string());
        }

        ensure_document_can_write_back(&session.document_path)?;
        if !session.write_back_supported {
            return Err(session
                .write_back_block_reason
                .clone()
                .unwrap_or_else(|| "当前文档暂不支持安全写回覆盖。".to_string()));
        }

        let target = PathBuf::from(&session.document_path);
        let mut content = rewrite_jobs::build_merged_text(&session);
        if is_docx_path(&target) {
            let bytes = load_verified_writeback_bytes(
                &target,
                &session.source_text,
                session.source_snapshot.as_ref(),
            )?;
            let updated_regions = rewrite_jobs::build_merged_regions(&session);
            let updated =
                DocxAdapter::write_updated_regions(&bytes, &session.source_text, &updated_regions)?;
            write_bytes_atomically(&target, &updated)?;
        } else {
            let line_ending = rewrite::detect_line_ending(&session.source_text);
            if !rewrite::has_trailing_spaces_per_line(&session.source_text) {
                content = rewrite::strip_trailing_spaces_per_line(&content);
            }
            content = rewrite::convert_line_endings(&content, line_ending);
            // 覆盖写回原文件：只写入“已应用”的修改，未应用的候选不会进入文件。
            write_document_content(
                &target,
                &session.source_text,
                session.source_snapshot.as_ref(),
                &content,
            )?;
        }

        // 写回成功后再清理记录，避免“写失败但记录被删”的风险。
        storage::delete_session(&app, &session_id)?;

        Ok(session.document_path)
    })
}
