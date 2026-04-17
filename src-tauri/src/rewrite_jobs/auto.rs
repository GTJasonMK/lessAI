use std::{collections::HashSet, sync::Arc};

use chrono::{DateTime, Utc};
use log::{error, info};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{
    models::{DocumentSession, RewriteFailedEvent, RunningState},
    rewrite_targets,
    session_access::access_current_session,
    state::{reserve_job, AppState, JobControl},
    storage,
};

use super::{
    emit_rewrite_finished, ensure_targets_available, resolve_available_rewrite_targets,
    rewrite_session_request,
};
use super::support::RewriteSessionAccess;
use crate::rewrite_jobs::auto_loop::run_auto_loop;

pub(crate) fn run_auto_rewrite(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: &str,
    target_rewrite_unit_ids: Option<Vec<String>>,
) -> Result<DocumentSession, String> {
    let (session, target_unit_ids, job) =
        start_auto_rewrite_session(&app, state.inner(), session_id, target_rewrite_unit_ids)?;
    spawn_auto_loop(&app, &session.id, target_unit_ids, job);
    Ok(session)
}

fn start_auto_rewrite_session(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
    target_rewrite_unit_ids: Option<Vec<String>>,
) -> Result<(DocumentSession, Option<HashSet<String>>, Arc<JobControl>), String> {
    access_current_session(
        rewrite_session_request(app, state, session_id, RewriteSessionAccess::ExternalEntry),
        |mut session| {
            let targets = resolve_available_rewrite_targets(&session, target_rewrite_unit_ids)?;
            let pending = rewrite_targets::build_auto_pending_queue(
                &session.rewrite_units,
                targets.target_unit_ids.as_ref(),
            );
            ensure_targets_available(pending, targets.has_target_subset, std::collections::VecDeque::is_empty)?;
            start_auto_rewrite_session_steps(
                &mut session,
                targets.target_unit_ids,
                |current_session_id| reserve_job(state, current_session_id),
                |current_session| storage::save_session(app, current_session),
                |current_session_id| crate::state::remove_job(state, current_session_id),
                Utc::now(),
            )
        },
    )
}

fn spawn_auto_loop(
    app: &AppHandle,
    session_id: &str,
    target_unit_ids: Option<HashSet<String>>,
    job: Arc<JobControl>,
) {
    let session_id = session_id.to_string();
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = run_auto_loop(app_handle.clone(), session_id.clone(), job, target_unit_ids)
            .await;
        match &result {
            Ok(()) => info!(
                "auto loop finished: session_id={} outcome=success remove_job_before_signal=true",
                session_id
            ),
            Err(message) => error!(
                "auto loop finished: session_id={} outcome=failed remove_job_before_signal=true error={message}",
                session_id
            ),
        }
        let state = app_handle.state::<AppState>();
        let finish_result = finish_spawned_auto_loop_steps(
            result,
            || crate::state::remove_job(&state, &session_id),
            || emit_rewrite_finished(&app_handle, &session_id),
            |error| {
                app_handle
                    .emit(
                        "rewrite_failed",
                        RewriteFailedEvent {
                            session_id: session_id.clone(),
                            error,
                        },
                    )
                    .map_err(|emit_error| emit_error.to_string())
            },
        );
        if let Err(message) = finish_result {
            error!(
                "auto loop finalization failed: session_id={} error={message}",
                session_id
            );
        }
    });
}

fn finish_spawned_auto_loop_steps<Remove, EmitFinished, EmitFailed>(
    result: Result<(), String>,
    remove_job: Remove,
    emit_finished: EmitFinished,
    emit_failed: EmitFailed,
) -> Result<(), String>
where
    Remove: FnOnce() -> Result<(), String>,
    EmitFinished: FnOnce() -> Result<(), String>,
    EmitFailed: FnOnce(String) -> Result<(), String>,
{
    remove_job()?;
    match result {
        Ok(()) => emit_finished(),
        Err(error) => emit_failed(error),
    }
}

fn start_auto_rewrite_session_steps<Reserve, Save, Rollback>(
    session: &mut DocumentSession,
    target_unit_ids: Option<HashSet<String>>,
    reserve: Reserve,
    save: Save,
    rollback: Rollback,
    updated_at: DateTime<Utc>,
) -> Result<(DocumentSession, Option<HashSet<String>>, Arc<JobControl>), String>
where
    Reserve: FnOnce(&str) -> Result<Arc<JobControl>, String>,
    Save: FnOnce(&DocumentSession) -> Result<(), String>,
    Rollback: FnOnce(&str) -> Result<(), String>,
{
    let job = reserve(&session.id)?;
    session.status = RunningState::Running;
    session.updated_at = updated_at;
    let saved_session = session.clone();
    if let Err(error) = save(&saved_session) {
        let _ = rollback(&session.id);
        return Err(error);
    }
    Ok((saved_session, target_unit_ids, job))
}

#[cfg(test)]
#[path = "auto_tests.rs"]
mod tests;
