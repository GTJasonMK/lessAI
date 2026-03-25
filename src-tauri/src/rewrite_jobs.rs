use std::{
    collections::{HashSet, VecDeque},
    path::Path,
    sync::{atomic::Ordering, Arc},
};

use chrono::Utc;
use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;

use crate::{
    documents::document_format,
    models::{
        ChunkCompletedEvent, ChunkStatus, DocumentSession, EditSuggestion, RewriteFailedEvent,
        RewriteMode, RewriteProgress, RunningState, SessionEvent, SuggestionDecision,
    },
    rewrite,
    state::{with_session_lock, AppState, JobControl},
    storage,
};

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

pub(crate) async fn prepare_session_for_rewrite(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
) -> Result<DocumentSession, String> {
    with_session_lock(state, session_id, || storage::load_session(app, session_id))
}

pub(crate) async fn run_manual_rewrite(
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
    with_session_lock(state, &session.id, || {
        storage::load_session(app, &session.id)
    })
}

pub(crate) fn run_auto_rewrite(
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
    with_session_lock(state.inner(), &session.id, || {
        storage::save_session(&app, &session)
    })?;

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
            let _ = crate::state::remove_job(&state, &session_id);
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

    let (format, total_chunks, mut pending, source_texts, mut completed_chunks) =
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

            let format = document_format(Path::new(&session.document_path));
            Ok((format, total, pending, sources, completed))
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

    let mut tasks: tokio::task::JoinSet<(usize, Result<String, String>)> =
        tokio::task::JoinSet::new();
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
            let format = format;
            tasks.spawn(async move {
                let result =
                    rewrite::rewrite_chunk_with_client(&client, &settings, &source_text, format)
                        .await;
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
            Ok(Some(joined)) => match joined {
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
            },
            Ok(None) => {
                in_flight_indices.clear();
            }
            Err(_) => {
                // timeout：用于轮询 cancel/pause 状态，避免 join_next 长时间阻塞。
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

pub(crate) async fn process_chunk(
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
        let format = document_format(Path::new(&session.document_path));
        Ok((chunk.source_text.clone(), format))
    })?;

    let (source_text, format) = source_text;

    match rewrite::rewrite_chunk(&settings, &source_text, format).await {
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
        let source_has_line_break =
            chunk.source_text.contains('\n') || chunk.source_text.contains('\r');
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
                if suggestion.chunk_index == index
                    && suggestion.decision == SuggestionDecision::Applied
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

fn mark_session_cancelled(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
) -> Result<(), String> {
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

pub(crate) fn update_session_status(
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

pub(crate) fn build_merged_text(session: &DocumentSession) -> String {
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
