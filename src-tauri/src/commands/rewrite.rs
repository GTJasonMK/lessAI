use std::sync::atomic::Ordering;

use tauri::{AppHandle, State};

use crate::{
    models::{DocumentSession, RewriteMode, RunningState},
    rewrite_jobs,
    state::{AppState, JobControl},
};

#[tauri::command]
pub async fn start_rewrite(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    mode: RewriteMode,
) -> Result<DocumentSession, String> {
    let session =
        rewrite_jobs::prepare_session_for_rewrite(&app, state.inner(), &session_id).await?;

    match mode {
        RewriteMode::Manual => {
            rewrite_jobs::run_manual_rewrite(&app, state.inner(), &session).await
        }
        RewriteMode::Auto => rewrite_jobs::run_auto_rewrite(app, state, session),
    }
}

#[tauri::command]
pub fn pause_rewrite(
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
    rewrite_jobs::update_session_status(&app, state.inner(), &session_id, RunningState::Paused)
}

#[tauri::command]
pub fn resume_rewrite(
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
    rewrite_jobs::update_session_status(&app, state.inner(), &session_id, RunningState::Running)
}

#[tauri::command]
pub fn cancel_rewrite(
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

    rewrite_jobs::update_session_status(&app, state.inner(), &session_id, RunningState::Cancelled)
}

#[tauri::command]
pub async fn retry_chunk(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    index: usize,
) -> Result<DocumentSession, String> {
    rewrite_jobs::process_chunk(&app, state.inner(), &session_id, index, false).await?;
    crate::state::with_session_lock(state.inner(), &session_id, || {
        crate::storage::load_session(&app, &session_id)
    })
}
