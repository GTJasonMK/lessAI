use tauri::AppHandle;

use crate::{
    rewrite_batch_commit::{
        batch_commit_mode, commit_rewrite_result, emit_rewrite_unit_completed_events,
    },
    rewrite_job_state::{mark_auto_batch_failed, mark_session_cancelled, mark_session_failed},
    rewrite_writeback::validate_candidate_batch_writeback,
    rewrite_unit::RewriteBatchResponse,
    state::AppState,
};

pub(super) type AutoTaskJoin = (Vec<String>, Result<RewriteBatchResponse, String>);

pub(super) const UNKNOWN_IN_FLIGHT_BATCH_ERROR: &str =
    "自动改写任务状态异常：收到未登记批次的完成结果。";
pub(super) const TASK_SET_DRAINED_WITH_IN_FLIGHT_BATCHES_ERROR: &str =
    "自动改写任务状态异常：后台任务集合已清空，但仍存在未完成批次。";

pub(super) enum AutoLoopStop<'a> {
    Cancelled,
    SessionFailed(String),
    SettledFailure(String),
    BatchFailed {
        rewrite_unit_ids: &'a [String],
        error: String,
    },
}

pub(super) fn commit_auto_batch(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
    rewrite_unit_ids: &[String],
    response: RewriteBatchResponse,
) -> Result<Vec<(String, String, u64)>, String> {
    let completed_batch = commit_rewrite_result(
        app,
        state,
        session_id,
        rewrite_unit_ids,
        Ok(response),
        batch_commit_mode(true),
        validate_candidate_batch_writeback,
    )?;
    emit_rewrite_unit_completed_events(app, session_id, &completed_batch)?;
    Ok(completed_batch)
}

pub(super) fn finish_auto_loop(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
    tasks: &mut tokio::task::JoinSet<AutoTaskJoin>,
    in_flight_batches: &mut Vec<Vec<String>>,
    stop: AutoLoopStop<'_>,
) -> Result<(), String> {
    abort_in_flight(tasks, in_flight_batches);
    match stop {
        AutoLoopStop::Cancelled => {
            mark_session_cancelled(app, state, session_id)?;
            Ok(())
        }
        AutoLoopStop::SessionFailed(error) => {
            mark_session_failed(app, state, session_id, error.clone())?;
            Err(error)
        }
        AutoLoopStop::SettledFailure(error) => Err(error),
        AutoLoopStop::BatchFailed {
            rewrite_unit_ids,
            error,
        } => {
            mark_auto_batch_failed(app, state, session_id, rewrite_unit_ids, error.clone())?;
            Err(error)
        }
    }
}

pub(super) fn ensure_in_flight_batches_drained(
    in_flight_batches: &[Vec<String>],
) -> Result<(), String> {
    if in_flight_batches.is_empty() {
        return Ok(());
    }
    Err(TASK_SET_DRAINED_WITH_IN_FLIGHT_BATCHES_ERROR.to_string())
}

pub(super) fn remove_in_flight_batch(
    in_flight_batches: &mut Vec<Vec<String>>,
    rewrite_unit_ids: &[String],
) -> Result<(), String> {
    let Some(position) = in_flight_batches
        .iter()
        .position(|batch| batch == rewrite_unit_ids)
    else {
        return Err(UNKNOWN_IN_FLIGHT_BATCH_ERROR.to_string());
    };
    in_flight_batches.remove(position);
    Ok(())
}

pub(super) fn abort_in_flight(
    tasks: &mut tokio::task::JoinSet<AutoTaskJoin>,
    in_flight_batches: &mut Vec<Vec<String>>,
) {
    tasks.abort_all();
    in_flight_batches.clear();
}
