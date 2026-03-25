use std::{
    fs,
    path::{Path, PathBuf},
};

use chrono::Utc;
use tauri::{AppHandle, State};

use crate::{
    documents::{
        document_format, document_session_id, ensure_document_can_write_back, load_document_source,
    },
    models::{ChunkStatus, ChunkTask, DocumentSession, RunningState},
    rewrite,
    state::{with_session_lock, AppState},
    storage,
};

#[tauri::command]
pub fn load_session(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
) -> Result<DocumentSession, String> {
    with_session_lock(state.inner(), &session_id, || {
        storage::load_session(&app, &session_id)
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
        if source_text.trim().is_empty() {
            return Err("文档内容为空。".to_string());
        }

        let normalized_text = rewrite::normalize_text(&source_text);
        let format = document_format(Path::new(&existing.document_path));
        let segmented = if let Some(regions) = loaded.regions {
            rewrite::segment_regions(regions, settings.chunk_preset)
        } else {
            rewrite::segment_text(
                &source_text,
                settings.chunk_preset,
                format,
                settings.rewrite_headings,
            )
        };
        let chunks = segmented
            .into_iter()
            .enumerate()
            .map(|(index, chunk)| ChunkTask {
                index,
                source_text: chunk.text,
                separator_after: chunk.separator_after,
                skip_rewrite: chunk.skip_rewrite,
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
            normalized_text,
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
        let existing = storage::load_session(&app, &session_id)?;

        ensure_document_can_write_back(&existing.document_path)?;

        let clean_session = existing.status == RunningState::Idle
            && existing.suggestions.is_empty()
            && existing
                .chunks
                .iter()
                .all(|chunk| chunk.status == ChunkStatus::Idle || chunk.skip_rewrite);

        if !clean_session {
            return Err(
                "该文档存在修订记录或进度，为避免冲突，请先“覆写并清理记录”或“重置记录”后再编辑。"
                    .to_string(),
            );
        }

        let line_ending = rewrite::detect_line_ending(&existing.source_text);
        let mut processed = content;
        if !rewrite::has_trailing_spaces_per_line(&existing.source_text) {
            processed = rewrite::strip_trailing_spaces_per_line(&processed);
        }
        processed = rewrite::convert_line_endings(&processed, line_ending);

        let target = PathBuf::from(&existing.document_path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::write(&target, &processed).map_err(|error| error.to_string())?;

        let settings = storage::load_settings(&app)?;
        let normalized_text = rewrite::normalize_text(&processed);
        let format = document_format(&target);
        let chunks = rewrite::segment_text(
            &processed,
            settings.chunk_preset,
            format,
            settings.rewrite_headings,
        )
        .into_iter()
        .enumerate()
        .map(|(index, chunk)| ChunkTask {
            index,
            source_text: chunk.text,
            separator_after: chunk.separator_after,
            skip_rewrite: chunk.skip_rewrite,
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
            source_text: processed,
            normalized_text,
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
pub fn open_document(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<DocumentSession, String> {
    if path.trim().is_empty() {
        return Err("文件路径不能为空。".to_string());
    }

    let canonical = fs::canonicalize(&path).map_err(|error| error.to_string())?;
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

            // 旧版本切块策略会 trim 掉片段边界的空格/换行，导致显示/导出时格式漂移。
            // 若该会话还没有任何修改对（suggestions 为空），则可以安全地用新算法重建 chunks，
            // 以保证 “chunks 拼回去 == source_text”。
            if session.suggestions.is_empty() {
                let settings = storage::load_settings(&app)?;
                let rebuilt = session
                    .chunks
                    .iter()
                    .map(|chunk| format!("{}{}", chunk.source_text, chunk.separator_after))
                    .collect::<String>();
                let fmt = document_format(Path::new(&session.document_path));
                // Sentence/Clause 预设下，chunk.source_text 通常不应包含换行：
                // - 换行属于格式，应被保存在 separator_after 中以保证“拼回原文不漂移”
                // - 例外 1：skip_rewrite chunk 允许包含多行（例如 Markdown fenced code block）
                // - 例外 2：TeX 的“单换行”在渲染层通常不代表段落边界；为了让 chunk 更贴近
                //   “渲染文本流”，Sentence/Clause 模式允许段内换行留在 chunk.source_text 里。
                let has_inline_newlines = session.chunks.iter().any(|chunk| {
                    !chunk.skip_rewrite
                        && (chunk.source_text.contains('\n') || chunk.source_text.contains('\r'))
                });
                let allow_inline_newlines = fmt == crate::models::DocumentFormat::Tex;

                // 迁移条件：
                // - 旧切块可能把换行包含进 chunk.source_text，导致 LLM 改写时合并/拆分行；
                // - 只要 suggestions 为空，就可以无损重建切块来锁定格式。
                //
                // 注意：段落模式允许 chunk.source_text 内含换行（段内换行），因此该条件仅用于
                // Sentence/Clause 模式的迁移检测。
                let should_rebuild = rebuilt != session.source_text
                    || (settings.chunk_preset != crate::models::ChunkPreset::Paragraph
                        && has_inline_newlines
                        && !allow_inline_newlines);

                if should_rebuild {
                    let normalized_text = rewrite::normalize_text(&session.source_text);
                    let chunks = rewrite::segment_text(
                        &session.source_text,
                        settings.chunk_preset,
                        fmt,
                        settings.rewrite_headings,
                    )
                    .into_iter()
                    .enumerate()
                    .map(|(index, chunk)| ChunkTask {
                        index,
                        source_text: chunk.text,
                        separator_after: chunk.separator_after,
                        skip_rewrite: chunk.skip_rewrite,
                        status: if chunk.skip_rewrite {
                            ChunkStatus::Done
                        } else {
                            ChunkStatus::Idle
                        },
                        error_message: None,
                    })
                    .collect::<Vec<_>>();

                    session.normalized_text = normalized_text;
                    session.chunks = chunks;
                    session.status = RunningState::Idle;
                    session.updated_at = Utc::now();
                    storage::save_session(&app, &session)?;
                }
            }
            return Ok(session);
        }

        let settings = storage::load_settings(&app)?;

        let loaded = load_document_source(&canonical, settings.rewrite_headings)?;
        let source_text = loaded.source_text;
        if source_text.trim().is_empty() {
            return Err("文档内容为空。".to_string());
        }

        let normalized_text = rewrite::normalize_text(&source_text);
        let format = document_format(&canonical);
        let segmented = if let Some(regions) = loaded.regions {
            rewrite::segment_regions(regions, settings.chunk_preset)
        } else {
            rewrite::segment_text(
                &source_text,
                settings.chunk_preset,
                format,
                settings.rewrite_headings,
            )
        };
        let chunks = segmented
            .into_iter()
            .enumerate()
            .map(|(index, chunk)| ChunkTask {
                index,
                source_text: chunk.text,
                separator_after: chunk.separator_after,
                skip_rewrite: chunk.skip_rewrite,
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
            normalized_text,
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
