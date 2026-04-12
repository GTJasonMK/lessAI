use std::{
    fs,
    path::{Path, PathBuf},
};

use chrono::Utc;
use tauri::{AppHandle, State};

use crate::{
    document_snapshot::capture_document_snapshot,
    documents::{
        document_format, document_session_id, load_document_source, write_document_content,
        LoadedDocumentSource,
    },
    editor_writeback::{
        ensure_session_can_use_plain_text_editor, normalize_editor_writeback_content,
    },
    models::{ChunkStatus, ChunkTask, DocumentSession, RunningState},
    rewrite,
    session_refresh::refresh_session_from_disk,
    session_repair::{repair_session_snapshot_if_needed, SnapshotRepairOutcome},
    state::{with_session_lock, AppState},
    storage,
};

pub(crate) fn rebuild_clean_session_from_disk(
    app: &AppHandle,
    existing: &DocumentSession,
) -> Result<DocumentSession, String> {
    let target = PathBuf::from(&existing.document_path);
    let settings = storage::load_settings(app)?;
    let loaded = load_document_source(&target, settings.rewrite_headings)?;
    let source_snapshot = Some(capture_document_snapshot(&target)?);

    Ok(build_clean_session_from_loaded(
        existing,
        &target,
        loaded,
        source_snapshot,
        settings.chunk_preset,
        settings.rewrite_headings,
    ))
}

fn build_clean_session_from_loaded(
    existing: &DocumentSession,
    target: &Path,
    loaded: LoadedDocumentSource,
    source_snapshot: Option<crate::models::DocumentSnapshot>,
    chunk_preset: crate::models::ChunkPreset,
    rewrite_headings: bool,
) -> DocumentSession {
    let LoadedDocumentSource {
        source_text,
        regions,
        region_segmentation_strategy,
        write_back_supported,
        write_back_block_reason,
        plain_text_editor_safe,
        plain_text_editor_block_reason,
    } = loaded;
    let normalized_text = rewrite::normalize_text(&source_text);
    let format = document_format(target);
    let segmented = rewrite::segment_regions_with_strategy(
        regions,
        chunk_preset,
        format,
        region_segmentation_strategy,
    );
    let chunks = segmented
        .into_iter()
        .enumerate()
        .map(|(index, chunk)| ChunkTask {
            index,
            source_text: chunk.text,
            separator_after: chunk.separator_after,
            skip_rewrite: chunk.skip_rewrite,
            presentation: chunk.presentation,
            status: if chunk.skip_rewrite {
                ChunkStatus::Done
            } else {
                ChunkStatus::Idle
            },
            error_message: None,
        })
        .collect::<Vec<_>>();

    let now = Utc::now();
    DocumentSession {
        id: existing.id.clone(),
        title: target
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("未命名文稿")
            .to_string(),
        document_path: existing.document_path.clone(),
        source_text,
        source_snapshot,
        normalized_text,
        write_back_supported,
        write_back_block_reason,
        plain_text_editor_safe,
        plain_text_editor_block_reason,
        chunk_preset: Some(chunk_preset),
        rewrite_headings: Some(rewrite_headings),
        chunks,
        suggestions: Vec::new(),
        next_suggestion_sequence: 1,
        status: RunningState::Idle,
        created_at: existing.created_at,
        updated_at: now,
    }
}

#[tauri::command]
pub fn load_session(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
) -> Result<DocumentSession, String> {
    with_session_lock(state.inner(), &session_id, || {
        let session = storage::load_session(&app, &session_id)?;
        let refreshed = refresh_session_from_disk(&app, &session)?;
        if refreshed.changed {
            storage::save_session(&app, &refreshed.session)?;
        }
        Ok(refreshed.session)
    })
}

#[tauri::command]
pub fn reset_session(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
) -> Result<DocumentSession, String> {
    {
        // 避免与后台 job 竞争写 session 文件；如果任务仍在运行或退出中，直接拒绝。
        let jobs = state
            .jobs
            .lock()
            .map_err(|_| "任务状态锁已损坏。".to_string())?;
        if jobs.contains_key(&session_id) {
            return Err("后台任务仍在运行或正在退出，请稍后再试。".to_string());
        }
    }

    with_session_lock(state.inner(), &session_id, || {
        let existing = storage::load_session(&app, &session_id)?;

        if matches!(
            existing.status,
            RunningState::Running | RunningState::Paused
        ) {
            return Err("当前文档正在执行自动任务，请先暂停并取消后再重置。".to_string());
        }

        let settings = storage::load_settings(&app)?;

        // 重置是“清空会话记录并重建切块”，不修改原文件。
        let loaded = load_document_source(
            Path::new(&existing.document_path),
            settings.rewrite_headings,
        )?;
        let source_text = loaded.source_text;
        let source_snapshot = Some(capture_document_snapshot(Path::new(
            &existing.document_path,
        ))?);
        let write_back_supported = loaded.write_back_supported;
        let write_back_block_reason = loaded.write_back_block_reason;
        let plain_text_editor_safe = loaded.plain_text_editor_safe;
        let plain_text_editor_block_reason = loaded.plain_text_editor_block_reason;
        if source_text.trim().is_empty() {
            return Err("文档内容为空。".to_string());
        }

        let normalized_text = rewrite::normalize_text(&source_text);
        let format = document_format(Path::new(&existing.document_path));
        let segmented = rewrite::segment_regions_with_strategy(
            loaded.regions,
            settings.chunk_preset,
            format,
            loaded.region_segmentation_strategy,
        );
        let chunks = segmented
            .into_iter()
            .enumerate()
            .map(|(index, chunk)| ChunkTask {
                index,
                source_text: chunk.text,
                separator_after: chunk.separator_after,
                skip_rewrite: chunk.skip_rewrite,
                presentation: chunk.presentation,
                status: if chunk.skip_rewrite {
                    ChunkStatus::Done
                } else {
                    ChunkStatus::Idle
                },
                error_message: None,
            })
            .collect::<Vec<_>>();

        let title = PathBuf::from(&existing.document_path)
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("未命名文稿")
            .to_string();

        let now = Utc::now();
        let session = DocumentSession {
            id: session_id.clone(),
            title,
            document_path: existing.document_path,
            source_text,
            source_snapshot,
            normalized_text,
            write_back_supported,
            write_back_block_reason,
            plain_text_editor_safe,
            plain_text_editor_block_reason,
            chunk_preset: Some(settings.chunk_preset),
            rewrite_headings: Some(settings.rewrite_headings),
            chunks,
            suggestions: Vec::new(),
            next_suggestion_sequence: 1,
            status: RunningState::Idle,
            created_at: now,
            updated_at: now,
        };

        storage::save_session(&app, &session)?;
        Ok(session)
    })
}

#[tauri::command]
pub fn save_document_edits(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    content: String,
) -> Result<DocumentSession, String> {
    if content.trim().is_empty() {
        return Err("文档内容为空，无法保存。".to_string());
    }

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

    let session_id_for_lock = session_id.clone();
    with_session_lock(state.inner(), &session_id_for_lock, move || {
        let mut existing = storage::load_session(&app, &session_id)?;
        let repair = repair_session_snapshot_if_needed(&app, &mut existing)?;
        if repair != SnapshotRepairOutcome::None {
            storage::save_session(&app, &existing)?;
        }
        if repair == SnapshotRepairOutcome::Rebuilt {
            return Err(
                "当前会话来自旧版本解析结果，系统已刷新到最新文档结构。当前编辑器内容未写入；请先复制你的修改，再重新进入编辑器后保存。"
                    .to_string(),
            );
        }

        ensure_session_can_use_plain_text_editor(&existing)?;
        if crate::documents::is_docx_path(Path::new(&existing.document_path)) {
            return Err("docx 编辑模式必须按片段保存，不能再走整篇纯文本写回。".to_string());
        }

        let target = PathBuf::from(&existing.document_path);
        let processed = normalize_editor_writeback_content(
            &existing.document_path,
            &existing.source_text,
            &content,
        );
        write_document_content(
            &target,
            &existing.source_text,
            existing.source_snapshot.as_ref(),
            &processed,
        )?;
        let session = rebuild_clean_session_from_disk(&app, &existing)?;

        storage::save_session(&app, &session)?;
        Ok(session)
    })
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
        if let Some(mut session) = storage::load_session_optional(&app, &session_id)? {
            // 进度恢复：如果上次崩溃/强退导致状态停留在 running/paused，这里统一降级，
            // 避免 UI 误以为还能继续后台任务（后台 job 在重启后不可恢复）。
            if matches!(session.status, RunningState::Running | RunningState::Paused) {
                session.status = RunningState::Cancelled;
                session.updated_at = Utc::now();
                storage::save_session(&app, &session)?;
            }
            let refreshed = refresh_session_from_disk(&app, &session)?;
            if refreshed.changed {
                storage::save_session(&app, &refreshed.session)?;
            }
            return Ok(refreshed.session);
        }

        let settings = storage::load_settings(&app)?;

        let loaded = load_document_source(&canonical, settings.rewrite_headings)?;
        let source_text = loaded.source_text;
        let source_snapshot = Some(capture_document_snapshot(&canonical)?);
        let write_back_supported = loaded.write_back_supported;
        let write_back_block_reason = loaded.write_back_block_reason;
        let plain_text_editor_safe = loaded.plain_text_editor_safe;
        let plain_text_editor_block_reason = loaded.plain_text_editor_block_reason;
        if source_text.trim().is_empty() {
            return Err("文档内容为空。".to_string());
        }

        let normalized_text = rewrite::normalize_text(&source_text);
        let format = document_format(&canonical);
        let segmented = rewrite::segment_regions_with_strategy(
            loaded.regions,
            settings.chunk_preset,
            format,
            loaded.region_segmentation_strategy,
        );
        let chunks = segmented
            .into_iter()
            .enumerate()
            .map(|(index, chunk)| ChunkTask {
                index,
                source_text: chunk.text,
                separator_after: chunk.separator_after,
                skip_rewrite: chunk.skip_rewrite,
                presentation: chunk.presentation,
                status: if chunk.skip_rewrite {
                    ChunkStatus::Done
                } else {
                    ChunkStatus::Idle
                },
                error_message: None,
            })
            .collect::<Vec<_>>();

        let title = canonical
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("未命名文稿")
            .to_string();

        let now = Utc::now();
        let session = DocumentSession {
            id: session_id.clone(),
            title,
            document_path: canonical_str,
            source_text,
            source_snapshot,
            normalized_text,
            write_back_supported,
            write_back_block_reason,
            plain_text_editor_safe,
            plain_text_editor_block_reason,
            chunk_preset: Some(settings.chunk_preset),
            rewrite_headings: Some(settings.rewrite_headings),
            chunks,
            suggestions: Vec::new(),
            next_suggestion_sequence: 1,
            status: RunningState::Idle,
            created_at: now,
            updated_at: now,
        };

        storage::save_session(&app, &session)?;
        Ok(session)
    })
}
