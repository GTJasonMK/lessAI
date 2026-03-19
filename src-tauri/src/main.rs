#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod models;
mod rewrite;
mod storage;

use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use chrono::Utc;
use models::{
    AppSettings, ChunkCompletedEvent, ChunkStatus, ChunkTask, DocumentSession, EditSuggestion,
    RewriteFailedEvent, RewriteMode, RewriteProgress, RunningState, SessionEvent, SuggestionDecision,
};
use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;

#[derive(Default)]
struct AppState {
    jobs: Mutex<HashMap<String, Arc<JobControl>>>,
    /// 会话文件读写锁（按 session_id 维度）。
    ///
    /// 为什么要有这把锁：
    /// - 会话是 JSON 文件，写入使用 truncate+write；如果 UI 在写入过程中并发读取，会出现 JSON 解析失败。
    /// - 自动批处理引入并发后，会有多个任务同时写入同一个 session 文件，必须串行化。
    session_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

#[derive(Default)]
struct JobControl {
    paused: AtomicBool,
    cancelled: AtomicBool,
}

fn document_session_id(document_path: &str) -> String {
    // 用 UUID v5 将“文档路径”稳定映射为 session id：
    // - 同一台机器上同一路径 => 同一个 id（用于恢复进度）
    // - 避免把路径直接当文件名（包含非法字符/过长）
    let namespace = Uuid::from_bytes([
        0x6c, 0x65, 0x73, 0x73, 0x61, 0x69, 0x2d, 0x64, 0x6f, 0x63, 0x2d, 0x6e, 0x73, 0x2d,
        0x30, 0x31,
    ]);
    Uuid::new_v5(&namespace, document_path.as_bytes()).to_string()
}

fn session_lock(state: &AppState, session_id: &str) -> Result<Arc<Mutex<()>>, String> {
    let mut locks = state
        .session_locks
        .lock()
        .map_err(|_| "会话锁状态已损坏。".to_string())?;

    Ok(locks
        .entry(session_id.to_string())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone())
}

fn with_session_lock<T>(
    state: &AppState,
    session_id: &str,
    f: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let lock = session_lock(state, session_id)?;
    let _guard = lock
        .lock()
        .map_err(|_| "会话锁已损坏（可能是上次进程异常退出）。".to_string())?;
    f()
}

const MAX_MAX_CONCURRENCY: usize = 8;

fn clamp_max_concurrency(value: usize) -> usize {
    value.clamp(1, MAX_MAX_CONCURRENCY)
}

fn snapshot_running_indices(in_flight: &HashSet<usize>) -> Vec<usize> {
    let mut indices = in_flight.iter().copied().collect::<Vec<_>>();
    indices.sort_unstable();
    indices
}

fn emit_rewrite_progress(
    app: &AppHandle,
    session_id: &str,
    completed_chunks: usize,
    running_indices: Vec<usize>,
    total_chunks: usize,
    mode: RewriteMode,
    running_state: RunningState,
    max_concurrency: usize,
) -> Result<(), String> {
    let in_flight = running_indices.len();
    app.emit(
        "rewrite_progress",
        RewriteProgress {
            session_id: session_id.to_string(),
            completed_chunks,
            in_flight,
            running_indices,
            total_chunks,
            mode,
            running_state,
            max_concurrency,
        },
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
fn load_settings(app: AppHandle) -> Result<AppSettings, String> {
    storage::load_settings(&app)
}

#[tauri::command]
fn save_settings(app: AppHandle, settings: AppSettings) -> Result<AppSettings, String> {
    storage::save_settings(&app, &settings)
}

#[tauri::command]
async fn test_provider(settings: AppSettings) -> Result<models::ProviderCheckResult, String> {
    rewrite::test_provider(&settings).await
}

#[tauri::command]
fn load_session(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
) -> Result<DocumentSession, String> {
    with_session_lock(state.inner(), &session_id, || {
        storage::load_session(&app, &session_id)
    })
}

#[tauri::command]
fn reset_session(
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

        if matches!(existing.status, RunningState::Running | RunningState::Paused) {
            return Err("当前文档正在执行自动任务，请先暂停并取消后再重置。".to_string());
        }

        // 重置是“清空会话记录并重建切块”，不修改原文件。
        let source_text =
            fs::read_to_string(&existing.document_path).map_err(|error| error.to_string())?;
        if source_text.trim().is_empty() {
            return Err("文档内容为空。".to_string());
        }

        let settings = storage::load_settings(&app)?;
        let normalized_text = rewrite::normalize_text(&source_text);
        let chunks = rewrite::segment_text(&source_text, settings.chunk_preset)
            .into_iter()
            .enumerate()
            .map(|(index, chunk)| ChunkTask {
                index,
                source_text: chunk.text,
                separator_after: chunk.separator_after,
                status: ChunkStatus::Idle,
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
fn open_document(
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
                let has_inline_newlines = session.chunks.iter().any(|chunk| {
                    chunk.source_text.contains('\n') || chunk.source_text.contains('\r')
                });

                // 迁移条件：
                // - 旧切块可能把换行包含进 chunk.source_text，导致 LLM 改写时合并/拆分行；
                // - 只要 suggestions 为空，就可以无损重建切块来锁定格式。
                //
                // 注意：段落模式允许 chunk.source_text 内含换行（段内换行），因此该条件仅用于
                // Sentence/Clause 模式的迁移检测。
                let should_rebuild = rebuilt != session.source_text
                    || (settings.chunk_preset != models::ChunkPreset::Paragraph
                        && has_inline_newlines);

                if should_rebuild {
                    let normalized_text = rewrite::normalize_text(&session.source_text);
                    let chunks = rewrite::segment_text(&session.source_text, settings.chunk_preset)
                        .into_iter()
                        .enumerate()
                        .map(|(index, chunk)| ChunkTask {
                            index,
                            source_text: chunk.text,
                            separator_after: chunk.separator_after,
                            status: ChunkStatus::Idle,
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

        let source_text = fs::read_to_string(&canonical).map_err(|error| error.to_string())?;
        if source_text.trim().is_empty() {
            return Err("文档内容为空。".to_string());
        }

        let settings = storage::load_settings(&app)?;
        let normalized_text = rewrite::normalize_text(&source_text);
        let chunks = rewrite::segment_text(&source_text, settings.chunk_preset)
            .into_iter()
            .enumerate()
            .map(|(index, chunk)| ChunkTask {
                index,
                source_text: chunk.text,
                separator_after: chunk.separator_after,
                status: ChunkStatus::Idle,
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

#[tauri::command]
async fn start_rewrite(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    mode: RewriteMode,
) -> Result<DocumentSession, String> {
    let session = with_session_lock(state.inner(), &session_id, || {
        storage::load_session(&app, &session_id)
    })?;

    match mode {
        RewriteMode::Manual => run_manual_rewrite(&app, state.inner(), &session).await,
        RewriteMode::Auto => run_auto_rewrite(app, state, session),
    }
}

#[tauri::command]
fn pause_rewrite(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
) -> Result<DocumentSession, String> {
    let job = {
        let jobs = state
            .jobs
            .lock()
            .map_err(|_| "任务状态锁已损坏。".to_string())?;
        jobs.get(&session_id)
            .cloned()
            .ok_or_else(|| "当前没有可暂停的任务。".to_string())?
    };

    job.paused.store(true, Ordering::SeqCst);
    update_session_status(&app, state.inner(), &session_id, RunningState::Paused)
}

#[tauri::command]
fn resume_rewrite(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
) -> Result<DocumentSession, String> {
    let job = {
        let jobs = state
            .jobs
            .lock()
            .map_err(|_| "任务状态锁已损坏。".to_string())?;
        jobs.get(&session_id)
            .cloned()
            .ok_or_else(|| "当前没有可继续的任务。".to_string())?
    };

    job.paused.store(false, Ordering::SeqCst);
    update_session_status(&app, state.inner(), &session_id, RunningState::Running)
}

#[tauri::command]
fn cancel_rewrite(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
) -> Result<DocumentSession, String> {
    let maybe_job = {
        let jobs = state
            .jobs
            .lock()
            .map_err(|_| "任务状态锁已损坏。".to_string())?;
        jobs.get(&session_id).cloned()
    };

    if let Some(job) = maybe_job {
        job.cancelled.store(true, Ordering::SeqCst);
    }

    update_session_status(&app, state.inner(), &session_id, RunningState::Cancelled)
}

#[tauri::command]
fn apply_suggestion(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    suggestion_id: String,
) -> Result<DocumentSession, String> {
    with_session_lock(state.inner(), &session_id, || {
        let mut session = storage::load_session(&app, &session_id)?;
        let now = Utc::now();

        let (chunk_index, found) = session
            .suggestions
            .iter()
            .find(|item| item.id == suggestion_id)
            .map(|item| (item.chunk_index, true))
            .unwrap_or((0, false));

        if !found {
            return Err("未找到对应的修改对。".to_string());
        }

        for suggestion in session.suggestions.iter_mut() {
            if suggestion.chunk_index != chunk_index {
                continue;
            }

            if suggestion.id == suggestion_id {
                suggestion.decision = SuggestionDecision::Applied;
                suggestion.updated_at = now;
            } else if suggestion.decision == SuggestionDecision::Applied {
                suggestion.decision = SuggestionDecision::Dismissed;
                suggestion.updated_at = now;
            }
        }

        session.updated_at = now;
        storage::save_session(&app, &session)?;
        Ok(session)
    })
}

#[tauri::command]
fn dismiss_suggestion(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    suggestion_id: String,
) -> Result<DocumentSession, String> {
    with_session_lock(state.inner(), &session_id, || {
        let mut session = storage::load_session(&app, &session_id)?;
        let now = Utc::now();
        let suggestion = session
            .suggestions
            .iter_mut()
            .find(|item| item.id == suggestion_id)
            .ok_or_else(|| "未找到对应的修改对。".to_string())?;

        suggestion.decision = SuggestionDecision::Dismissed;
        suggestion.updated_at = now;
        session.updated_at = now;
        storage::save_session(&app, &session)?;
        Ok(session)
    })
}

#[tauri::command]
fn delete_suggestion(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    suggestion_id: String,
) -> Result<DocumentSession, String> {
    with_session_lock(state.inner(), &session_id, || {
        let mut session = storage::load_session(&app, &session_id)?;
        let now = Utc::now();

        let removed = session
            .suggestions
            .iter()
            .find(|item| item.id == suggestion_id)
            .map(|item| item.chunk_index);

        session.suggestions.retain(|item| item.id != suggestion_id);

        if let Some(chunk_index) = removed {
            let still_has_any = session
                .suggestions
                .iter()
                .any(|item| item.chunk_index == chunk_index);

            if !still_has_any {
                if let Some(chunk) = session.chunks.get_mut(chunk_index) {
                    if chunk.status == ChunkStatus::Done {
                        chunk.status = ChunkStatus::Idle;
                    }
                }
            }
        }

        session.updated_at = now;
        storage::save_session(&app, &session)?;
        Ok(session)
    })
}

#[tauri::command]
async fn retry_chunk(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    index: usize,
) -> Result<DocumentSession, String> {
    process_chunk(&app, state.inner(), &session_id, index, false).await?;
    with_session_lock(state.inner(), &session_id, || {
        storage::load_session(&app, &session_id)
    })
}

#[tauri::command]
fn export_document(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> Result<String, String> {
    let session = with_session_lock(state.inner(), &session_id, || {
        storage::load_session(&app, &session_id)
    })?;
    let line_ending = rewrite::detect_line_ending(&session.source_text);
    let mut content = build_merged_text(&session);
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
fn finalize_document(
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

        let line_ending = rewrite::detect_line_ending(&session.source_text);
        let mut content = build_merged_text(&session);
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

async fn run_manual_rewrite(
    app: &AppHandle,
    state: &AppState,
    session: &DocumentSession,
) -> Result<DocumentSession, String> {
    if session.status == RunningState::Running || session.status == RunningState::Paused {
        return Err("当前文档正在执行自动任务，请先暂停或取消。".to_string());
    }

    let next_chunk = session
        .chunks
        .iter()
        .find(|chunk| matches!(chunk.status, ChunkStatus::Idle | ChunkStatus::Failed))
        .map(|chunk| chunk.index)
        .ok_or_else(|| "没有可继续处理的片段，当前文档可能已经全部完成。".to_string())?;

    process_chunk(app, state, &session.id, next_chunk, false).await?;
    with_session_lock(state, &session.id, || storage::load_session(app, &session.id))
}

fn run_auto_rewrite(
    app: AppHandle,
    state: State<'_, AppState>,
    mut session: DocumentSession,
) -> Result<DocumentSession, String> {
    {
        let jobs = state
            .jobs
            .lock()
            .map_err(|_| "任务状态锁已损坏。".to_string())?;
        if jobs.contains_key(&session.id) {
            return Err("当前会话已经存在运行中的任务。".to_string());
        }
    }

    session.status = RunningState::Running;
    session.updated_at = Utc::now();
    with_session_lock(state.inner(), &session.id, || storage::save_session(&app, &session))?;

    {
        let mut jobs = state
            .jobs
            .lock()
            .map_err(|_| "任务状态锁已损坏。".to_string())?;

        let job = Arc::new(JobControl::default());
        jobs.insert(session.id.clone(), job.clone());
        let session_id = session.id.clone();
        let app_handle = app.clone();

        tauri::async_runtime::spawn(async move {
            let result = run_auto_loop(app_handle.clone(), session_id.clone(), job.clone()).await;
            if let Err(error) = result {
                let _ = app_handle.emit(
                    "rewrite_failed",
                    RewriteFailedEvent {
                        session_id: session_id.clone(),
                        error,
                    },
                );
            }

            let state = app_handle.state::<AppState>();
            let _ = remove_job(&state, &session_id);
        });
    }

    Ok(session)
}

async fn run_auto_loop(
    app: AppHandle,
    session_id: String,
    job: Arc<JobControl>,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    let app_state = state.inner();

    let settings = storage::load_settings(&app)?;
    let max_concurrency = clamp_max_concurrency(settings.max_concurrency);
    let client = Arc::new(rewrite::build_client(&settings)?);

    let (total_chunks, mut pending, source_texts, mut completed_chunks) =
        with_session_lock(app_state, &session_id, || {
            let mut session = storage::load_session(&app, &session_id)?;

            // 清理残留状态：崩溃/强退后可能会留下一些 running 片段。
            let mut touched = false;
            for chunk in session.chunks.iter_mut() {
                if chunk.status == ChunkStatus::Running {
                    chunk.status = ChunkStatus::Idle;
                    chunk.error_message = None;
                    touched = true;
                }
            }
            if touched {
                session.updated_at = Utc::now();
                storage::save_session(&app, &session)?;
            }

            let total = session.chunks.len();
            let completed = session
                .chunks
                .iter()
                .filter(|chunk| chunk.status == ChunkStatus::Done)
                .count();
            let pending = session
                .chunks
                .iter()
                .filter(|chunk| chunk.status != ChunkStatus::Done)
                .map(|chunk| chunk.index)
                .collect::<VecDeque<_>>();
            let sources = session
                .chunks
                .iter()
                .map(|chunk| chunk.source_text.clone())
                .collect::<Vec<_>>();

            Ok((total, pending, sources, completed))
        })?;

    emit_rewrite_progress(
        &app,
        &session_id,
        completed_chunks,
        Vec::new(),
        total_chunks,
        RewriteMode::Auto,
        if job.paused.load(Ordering::SeqCst) {
            RunningState::Paused
        } else {
            RunningState::Running
        },
        max_concurrency,
    )?;

    if pending.is_empty() {
        finalize_auto_session(&app, app_state, &session_id)?;
        app.emit(
            "rewrite_finished",
            SessionEvent {
                session_id: session_id.clone(),
            },
        )
        .map_err(|error| error.to_string())?;
        return Ok(());
    }

    let mut tasks: tokio::task::JoinSet<(usize, Result<String, String>)> = tokio::task::JoinSet::new();
    let mut in_flight_indices = HashSet::<usize>::new();

    loop {
        if job.cancelled.load(Ordering::SeqCst) {
            tasks.abort_all();
            in_flight_indices.clear();
            mark_session_cancelled(&app, app_state, &session_id)?;
            app.emit(
                "rewrite_finished",
                SessionEvent {
                    session_id: session_id.clone(),
                },
            )
            .map_err(|error| error.to_string())?;
            return Ok(());
        }

        while !job.paused.load(Ordering::SeqCst) && in_flight_indices.len() < max_concurrency {
            let Some(index) = pending.pop_front() else {
                break;
            };

            mark_chunk_running(&app, app_state, &session_id, index)?;
            in_flight_indices.insert(index);

            let source_text = source_texts
                .get(index)
                .cloned()
                .ok_or_else(|| "片段索引越界。".to_string())?;
            let client = client.clone();
            let settings = settings.clone();
            tasks.spawn(async move {
                let result =
                    rewrite::rewrite_chunk_with_client(&client, &settings, &source_text).await;
                (index, result)
            });

            emit_rewrite_progress(
                &app,
                &session_id,
                completed_chunks,
                snapshot_running_indices(&in_flight_indices),
                total_chunks,
                RewriteMode::Auto,
                if job.paused.load(Ordering::SeqCst) {
                    RunningState::Paused
                } else {
                    RunningState::Running
                },
                max_concurrency,
            )?;
        }

        if pending.is_empty() && in_flight_indices.is_empty() {
            break;
        }

        if in_flight_indices.is_empty() {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            continue;
        }

        match tokio::time::timeout(std::time::Duration::from_millis(250), tasks.join_next()).await {
            Ok(Some(joined)) => {
                match joined {
                    Ok((index, Ok(candidate_text))) => {
                        in_flight_indices.remove(&index);
                        let (suggestion_id, suggestion_sequence) = commit_chunk_success(
                            &app,
                            app_state,
                            &session_id,
                            index,
                            candidate_text,
                            SuggestionDecision::Applied,
                            None,
                        )?;
                        completed_chunks = completed_chunks.saturating_add(1);
                        app.emit(
                            "chunk_completed",
                            ChunkCompletedEvent {
                                session_id: session_id.clone(),
                                index,
                                suggestion_id,
                                suggestion_sequence,
                            },
                        )
                        .map_err(|error| error.to_string())?;
                    }
                    Ok((index, Err(error))) => {
                        tasks.abort_all();
                        in_flight_indices.remove(&index);
                        in_flight_indices.clear();
                        commit_chunk_failure(&app, app_state, &session_id, index, error.clone())?;
                        reset_running_chunks_to_idle(&app, app_state, &session_id)?;
                        return Err(error);
                    }
                    Err(join_error) => {
                        tasks.abort_all();
                        in_flight_indices.clear();
                        let error = format!("后台任务异常退出：{join_error}");
                        mark_session_failed(&app, app_state, &session_id, error.clone())?;
                        return Err(error);
                    }
                }

                emit_rewrite_progress(
                    &app,
                    &session_id,
                    completed_chunks,
                    snapshot_running_indices(&in_flight_indices),
                    total_chunks,
                    RewriteMode::Auto,
                    if job.paused.load(Ordering::SeqCst) {
                        RunningState::Paused
                    } else {
                        RunningState::Running
                    },
                    max_concurrency,
                )?;
            }
            Ok(None) => {
                in_flight_indices.clear();
            }
            Err(_) => {
                // timeout：用于轮询 cancel/pause 状态，避免 join_next 长时间阻塞。
            }
        }
    }

    finalize_auto_session(&app, app_state, &session_id)?;
    app.emit(
        "rewrite_finished",
        SessionEvent {
            session_id: session_id.clone(),
        },
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

async fn process_chunk(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
    index: usize,
    auto_approve: bool,
) -> Result<(), String> {
    let settings = storage::load_settings(app)?;
    mark_chunk_running(app, state, session_id, index)?;

    let source_text = with_session_lock(state, session_id, || {
        let session = storage::load_session(app, session_id)?;
        let chunk = session
            .chunks
            .get(index)
            .ok_or_else(|| "片段索引越界。".to_string())?;
        Ok(chunk.source_text.clone())
    })?;

    match rewrite::rewrite_chunk(&settings, &source_text).await {
        Ok(candidate_text) => {
            let decision = if auto_approve {
                SuggestionDecision::Applied
            } else {
                SuggestionDecision::Proposed
            };
            let set_status = if auto_approve {
                None
            } else {
                Some(RunningState::Idle)
            };

            let (suggestion_id, suggestion_sequence) = commit_chunk_success(
                app,
                state,
                session_id,
                index,
                candidate_text,
                decision,
                set_status,
            )?;

            app.emit(
                "chunk_completed",
                ChunkCompletedEvent {
                    session_id: session_id.to_string(),
                    index,
                    suggestion_id,
                    suggestion_sequence,
                },
            )
            .map_err(|error| error.to_string())?;

            Ok(())
        }
        Err(error) => {
            commit_chunk_failure(app, state, session_id, index, error.clone())?;
            Err(error)
        }
    }
}

fn commit_chunk_success(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
    index: usize,
    candidate_text: String,
    decision: SuggestionDecision,
    set_status: Option<RunningState>,
) -> Result<(String, u64), String> {
    with_session_lock(state, session_id, || {
        let mut latest = storage::load_session(app, session_id)?;
        let chunk = latest
            .chunks
            .get_mut(index)
            .ok_or_else(|| "片段索引越界。".to_string())?;

        let line_ending = rewrite::detect_line_ending(&latest.source_text);
        let mut candidate_text = candidate_text;
        if !rewrite::has_trailing_spaces_per_line(&chunk.source_text) {
            candidate_text = rewrite::strip_trailing_spaces_per_line(&candidate_text);
        }
        let source_has_line_break = chunk.source_text.contains('\n') || chunk.source_text.contains('\r');
        if !source_has_line_break {
            candidate_text = rewrite::collapse_line_breaks_to_spaces(&candidate_text);
        }
        candidate_text = rewrite::convert_line_endings(&candidate_text, line_ending);

        let now = Utc::now();
        let suggestion_id = Uuid::new_v4().to_string();
        let suggestion_sequence = latest.next_suggestion_sequence;
        latest.next_suggestion_sequence = latest.next_suggestion_sequence.saturating_add(1);

        if decision == SuggestionDecision::Applied {
            for suggestion in latest.suggestions.iter_mut() {
                if suggestion.chunk_index == index && suggestion.decision == SuggestionDecision::Applied
                {
                    suggestion.decision = SuggestionDecision::Dismissed;
                    suggestion.updated_at = now;
                }
            }
        }

        latest.suggestions.push(EditSuggestion {
            id: suggestion_id.clone(),
            sequence: suggestion_sequence,
            chunk_index: index,
            before_text: chunk.source_text.clone(),
            after_text: candidate_text.clone(),
            diff_spans: rewrite::build_diff(&chunk.source_text, &candidate_text),
            decision,
            created_at: now,
            updated_at: now,
        });

        chunk.status = ChunkStatus::Done;
        chunk.error_message = None;
        latest.updated_at = now;
        if let Some(status) = set_status {
            latest.status = status;
        }

        storage::save_session(app, &latest)?;
        Ok((suggestion_id, suggestion_sequence))
    })
}

fn commit_chunk_failure(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
    index: usize,
    error: String,
) -> Result<(), String> {
    with_session_lock(state, session_id, || {
        let mut latest = storage::load_session(app, session_id)?;
        let chunk = latest
            .chunks
            .get_mut(index)
            .ok_or_else(|| "片段索引越界。".to_string())?;
        chunk.status = ChunkStatus::Failed;
        chunk.error_message = Some(error.clone());
        latest.updated_at = Utc::now();
        latest.status = RunningState::Failed;
        storage::save_session(app, &latest)
    })
}

fn reset_running_chunks_to_idle(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
) -> Result<(), String> {
    with_session_lock(state, session_id, || {
        let mut session = storage::load_session(app, session_id)?;
        let mut touched = false;
        for chunk in session.chunks.iter_mut() {
            if chunk.status == ChunkStatus::Running {
                chunk.status = ChunkStatus::Idle;
                chunk.error_message = None;
                touched = true;
            }
        }
        if touched {
            session.updated_at = Utc::now();
            storage::save_session(app, &session)?;
        }
        Ok(())
    })
}

fn mark_session_cancelled(app: &AppHandle, state: &AppState, session_id: &str) -> Result<(), String> {
    with_session_lock(state, session_id, || {
        let mut session = storage::load_session(app, session_id)?;
        session.status = RunningState::Cancelled;
        for chunk in session.chunks.iter_mut() {
            if chunk.status == ChunkStatus::Running {
                chunk.status = ChunkStatus::Idle;
                chunk.error_message = None;
            }
        }
        session.updated_at = Utc::now();
        storage::save_session(app, &session)
    })
}

fn finalize_auto_session(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
) -> Result<RunningState, String> {
    with_session_lock(state, session_id, || {
        let mut session = storage::load_session(app, session_id)?;
        session.status = compute_session_state(&session);
        session.updated_at = Utc::now();
        storage::save_session(app, &session)?;
        Ok(session.status)
    })
}

fn mark_chunk_running(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
    index: usize,
) -> Result<(), String> {
    with_session_lock(state, session_id, || {
        let mut session = storage::load_session(app, session_id)?;
        let chunk = session
            .chunks
            .get_mut(index)
            .ok_or_else(|| "片段索引越界。".to_string())?;
        chunk.status = ChunkStatus::Running;
        chunk.error_message = None;
        session.updated_at = Utc::now();
        if session.status != RunningState::Paused {
            session.status = RunningState::Running;
        }
        storage::save_session(app, &session)
    })
}

fn update_session_status(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
    status: RunningState,
) -> Result<DocumentSession, String> {
    with_session_lock(state, session_id, || {
        let mut session = storage::load_session(app, session_id)?;
        session.status = status;
        session.updated_at = Utc::now();
        storage::save_session(app, &session)?;
        Ok(session)
    })
}

fn mark_session_failed(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
    error: String,
) -> Result<(), String> {
    with_session_lock(state, session_id, || {
        let mut session = storage::load_session(app, session_id)?;
        session.status = RunningState::Failed;
        session.updated_at = Utc::now();

        for chunk in session.chunks.iter_mut() {
            if chunk.status != ChunkStatus::Running {
                continue;
            }
            chunk.status = ChunkStatus::Failed;
            chunk.error_message = Some(error.clone());
        }

        storage::save_session(app, &session)
    })
}

fn compute_session_state(session: &DocumentSession) -> RunningState {
    if session
        .chunks
        .iter()
        .any(|chunk| chunk.status == ChunkStatus::Failed)
    {
        return RunningState::Failed;
    }

    let all_done = session
        .chunks
        .iter()
        .all(|chunk| chunk.status == ChunkStatus::Done);

    if all_done {
        return RunningState::Completed;
    }

    RunningState::Idle
}

fn build_merged_text(session: &DocumentSession) -> String {
    let mut merged = String::new();

    for chunk in session.chunks.iter() {
        let applied = session
            .suggestions
            .iter()
            .filter(|item| {
                item.chunk_index == chunk.index && item.decision == SuggestionDecision::Applied
            })
            .max_by_key(|item| item.sequence);
        let body = applied
            .map(|item| item.after_text.as_str())
            .unwrap_or(chunk.source_text.as_str());

        merged.push_str(body);
        merged.push_str(&chunk.separator_after);
    }

    merged
}

fn remove_job(state: &AppState, session_id: &str) -> Result<(), String> {
    let mut jobs = state
        .jobs
        .lock()
        .map_err(|_| "任务状态锁已损坏。".to_string())?;
    jobs.remove(session_id);
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .manage(AppState::default())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            load_settings,
            save_settings,
            test_provider,
            open_document,
            load_session,
            reset_session,
            start_rewrite,
            pause_rewrite,
            resume_rewrite,
            cancel_rewrite,
            apply_suggestion,
            dismiss_suggestion,
            delete_suggestion,
            retry_chunk,
            export_document,
            finalize_document
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
