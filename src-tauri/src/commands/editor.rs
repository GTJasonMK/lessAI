use std::path::{Path, PathBuf};

use tauri::{AppHandle, State};

use super::session::rebuild_clean_session_from_disk;
use crate::{
    document_edit_validation::validate_document_content_writeback,
    documents::{is_docx_path, write_document_content},
    editor_writeback::{
        build_updated_text_from_chunk_edits, ensure_session_can_use_plain_text_editor,
        normalize_editor_writeback_content,
    },
    models::{DocumentSession, EditorChunkEdit},
    session_repair::{
        refresh_session_capabilities_if_needed, repair_session_snapshot_if_needed,
        SnapshotRepairOutcome,
    },
    state::{with_session_lock, AppState},
    storage,
};

#[tauri::command]
pub fn validate_document_edits(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    content: String,
) -> Result<(), String> {
    if content.trim().is_empty() {
        return Err("文档内容为空，无法保存。".to_string());
    }

    ensure_no_running_job(state.inner(), &session_id)?;

    let session_id_for_lock = session_id.clone();
    with_session_lock(state.inner(), &session_id_for_lock, move || {
        let existing = load_repaired_editor_session(&app, &session_id)?;
        ensure_session_can_use_plain_text_editor(&existing)?;
        if is_docx_path(Path::new(&existing.document_path)) {
            return Err("docx 编辑模式必须按片段校验保存，不能再走整篇纯文本校验。".to_string());
        }
        let processed = normalize_editor_writeback_content(
            &existing.document_path,
            &existing.source_text,
            &content,
        );
        let target = PathBuf::from(&existing.document_path);
        validate_document_content_writeback(
            &target,
            &existing.source_text,
            existing.source_snapshot.as_ref(),
            &processed,
        )
    })
}

#[tauri::command]
pub fn validate_document_chunk_edits(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    edits: Vec<EditorChunkEdit>,
) -> Result<(), String> {
    ensure_no_running_job(state.inner(), &session_id)?;

    let session_id_for_lock = session_id.clone();
    with_session_lock(state.inner(), &session_id_for_lock, move || {
        let existing = load_repaired_editor_session(&app, &session_id)?;
        let updated_text = build_updated_text_from_chunk_edits(&existing, &edits)?;
        let target = PathBuf::from(&existing.document_path);
        validate_document_content_writeback(
            &target,
            &existing.source_text,
            existing.source_snapshot.as_ref(),
            &updated_text,
        )
    })
}

#[tauri::command]
pub fn save_document_chunk_edits(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    edits: Vec<EditorChunkEdit>,
) -> Result<DocumentSession, String> {
    ensure_no_running_job(state.inner(), &session_id)?;

    let session_id_for_lock = session_id.clone();
    with_session_lock(state.inner(), &session_id_for_lock, move || {
        let existing = load_repaired_editor_session(&app, &session_id)?;
        let updated_text = build_updated_text_from_chunk_edits(&existing, &edits)?;
        let target = PathBuf::from(&existing.document_path);
        write_document_content(
            &target,
            &existing.source_text,
            existing.source_snapshot.as_ref(),
            &updated_text,
        )?;

        let session = rebuild_clean_session_from_disk(&app, &existing)?;
        storage::save_session(&app, &session)?;
        Ok(session)
    })
}

fn ensure_no_running_job(state: &AppState, session_id: &str) -> Result<(), String> {
    let jobs = state
        .jobs
        .lock()
        .map_err(|_| "任务状态锁已损坏。".to_string())?;
    if jobs.contains_key(session_id) {
        return Err("后台任务仍在运行或正在退出，请稍后再试。".to_string());
    }
    Ok(())
}

fn load_repaired_editor_session(
    app: &AppHandle,
    session_id: &str,
) -> Result<DocumentSession, String> {
    let mut existing = storage::load_session(app, session_id)?;
    let repair = repair_session_snapshot_if_needed(app, &mut existing)?;
    let refreshed = refresh_session_capabilities_if_needed(&mut existing)?;
    if repair != SnapshotRepairOutcome::None || refreshed {
        storage::save_session(app, &existing)?;
    }
    if repair == SnapshotRepairOutcome::Rebuilt {
        return Err(
            "当前会话来自旧版本解析结果，系统已刷新到最新文档结构。当前编辑器内容未写入；请先复制你的修改，再重新进入编辑器后保存。"
                .to_string(),
        );
    }
    Ok(existing)
}
