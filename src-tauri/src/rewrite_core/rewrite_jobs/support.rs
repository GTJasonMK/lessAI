use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::Path,
    sync::atomic::Ordering,
};

use tauri::{AppHandle, Emitter};
use uuid::Uuid;

use crate::{
    documents::document_format,
    models::{DocumentSession, RewriteMode, RewriteProgress, RunningState, SessionEvent},
    rewrite_permissions::{
        ensure_session_can_rewrite, protected_rewrite_unit_error, REWRITE_UNIT_NOT_FOUND_ERROR,
    },
    rewrite_targets,
    rewrite_unit::{build_rewrite_unit_request, RewriteBatchRequest, RewriteUnitRequest},
    session_access::CurrentSessionRequest,
    session_messages::ACTIVE_REWRITE_SESSION_ERROR,
    state::{AppState, JobControl},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RewriteSessionAccess {
    ExternalEntry,
    ActiveJob,
}

pub(super) struct AvailableRewriteTargets {
    pub(super) target_unit_ids: Option<HashSet<String>>,
    pub(super) has_target_subset: bool,
}

pub(super) struct PreparedAutoRewriteSession {
    pub(super) total_units: usize,
    pub(super) pending: VecDeque<String>,
    pub(super) request_snapshot: HashMap<String, RewriteUnitRequest>,
    pub(super) completed_units: usize,
}

pub(super) struct PreparedLoadedRewriteBatch {
    pub(super) rewrite_unit_ids: Vec<String>,
    pub(super) batch_request: RewriteBatchRequest,
}

type RewriteSessionGuard = fn(&DocumentSession) -> Result<(), String>;
type RewriteSessionRequest<'a> = CurrentSessionRequest<'a, RewriteSessionGuard>;

pub(super) struct RewriteProgressEvent<'a> {
    pub(super) session_id: &'a str,
    pub(super) completed_units: usize,
    pub(super) in_flight: usize,
    pub(super) running_unit_ids: Vec<String>,
    pub(super) total_units: usize,
    pub(super) mode: RewriteMode,
    pub(super) running_state: RunningState,
    pub(super) max_concurrency: usize,
}

pub(super) fn rewrite_session_request<'a>(
    app: &'a AppHandle,
    state: &'a AppState,
    session_id: &'a str,
    access: RewriteSessionAccess,
) -> RewriteSessionRequest<'a> {
    let request = CurrentSessionRequest::guarded_refresh(
        app,
        state,
        session_id,
        ensure_session_can_rewrite as fn(&DocumentSession) -> Result<(), String>,
    );
    match rewrite_session_active_job_error(access) {
        Some(active_job_error) => request.with_active_job_error(active_job_error),
        None => request,
    }
}

pub(super) fn rewrite_session_active_job_error(
    access: RewriteSessionAccess,
) -> Option<&'static str> {
    match access {
        RewriteSessionAccess::ExternalEntry => Some(ACTIVE_REWRITE_SESSION_ERROR),
        RewriteSessionAccess::ActiveJob => None,
    }
}

pub(super) fn build_rewrite_source_snapshot(
    session: &DocumentSession,
) -> Result<HashMap<String, RewriteUnitRequest>, String> {
    let format = document_format(Path::new(&session.document_path));
    session
        .rewrite_units
        .iter()
        .map(|unit| {
            build_rewrite_unit_request(session, &unit.id, format)
                .map(|request| (unit.id.clone(), request))
        })
        .collect()
}

pub(super) fn collect_rewrite_batch_source_texts(
    source_snapshot: &HashMap<String, RewriteUnitRequest>,
    rewrite_unit_ids: &[String],
) -> Result<Vec<RewriteUnitRequest>, String> {
    rewrite_unit_ids
        .iter()
        .map(|rewrite_unit_id| {
            let request = source_snapshot
                .get(rewrite_unit_id)
                .ok_or_else(|| REWRITE_UNIT_NOT_FOUND_ERROR.to_string())?;
            let editable = request.slots.iter().any(|slot| slot.editable);
            if !editable {
                return Err(protected_rewrite_unit_error(rewrite_unit_id));
            }
            Ok(request.clone())
        })
        .collect()
}

pub(super) fn prepare_auto_rewrite_session(
    session: &DocumentSession,
    target_unit_ids: Option<&HashSet<String>>,
) -> Result<PreparedAutoRewriteSession, String> {
    Ok(PreparedAutoRewriteSession {
        total_units: rewrite_targets::count_target_total_units(
            &session.rewrite_units,
            target_unit_ids,
        ),
        pending: rewrite_targets::build_auto_pending_queue(&session.rewrite_units, target_unit_ids),
        request_snapshot: build_rewrite_source_snapshot(session)?,
        completed_units: rewrite_targets::count_target_completed_units(
            &session.rewrite_units,
            target_unit_ids,
        ),
    })
}

pub(super) fn prepare_loaded_rewrite_batch(
    session: &DocumentSession,
    rewrite_unit_ids: &[String],
) -> Result<PreparedLoadedRewriteBatch, String> {
    let source_snapshot = build_rewrite_source_snapshot(session)?;
    let requests = collect_rewrite_batch_source_texts(&source_snapshot, rewrite_unit_ids)?;
    Ok(PreparedLoadedRewriteBatch {
        rewrite_unit_ids: rewrite_unit_ids.to_vec(),
        batch_request: build_rewrite_batch_request(requests)?,
    })
}

pub(super) fn build_rewrite_batch_request(
    requests: Vec<RewriteUnitRequest>,
) -> Result<RewriteBatchRequest, String> {
    let format = requests
        .first()
        .map(|request| request.format.clone())
        .ok_or_else(|| "改写批次不包含任何单元。".to_string())?;
    Ok(RewriteBatchRequest::new(
        &Uuid::new_v4().to_string(),
        &format,
        requests,
    ))
}

pub(super) fn snapshot_running_indices_from_batches(
    in_flight_batches: &[Vec<String>],
) -> Vec<String> {
    let mut unit_ids = in_flight_batches
        .iter()
        .flat_map(|batch| batch.iter().cloned())
        .collect::<Vec<_>>();
    unit_ids.sort();
    unit_ids.dedup();
    unit_ids
}

pub(super) fn in_flight_batch_count(in_flight_batches: &[Vec<String>]) -> usize {
    in_flight_batches.len()
}

fn no_available_targets_error(has_target_subset: bool) -> String {
    if has_target_subset {
        "所选改写单元已处理完成。".to_string()
    } else {
        "没有可继续处理的改写单元，当前文档可能已经全部完成。".to_string()
    }
}

pub(super) fn resolve_available_rewrite_targets(
    session: &DocumentSession,
    target_rewrite_unit_ids: Option<Vec<String>>,
) -> Result<AvailableRewriteTargets, String> {
    let target_unit_ids = rewrite_targets::resolve_target_rewrite_unit_ids(
        &session.rewrite_units,
        target_rewrite_unit_ids,
    )?;
    Ok(AvailableRewriteTargets {
        has_target_subset: target_unit_ids.is_some(),
        target_unit_ids,
    })
}

pub(super) fn ensure_targets_available<T>(
    targets: T,
    has_target_subset: bool,
    is_empty: impl FnOnce(&T) -> bool,
) -> Result<T, String> {
    if is_empty(&targets) {
        return Err(no_available_targets_error(has_target_subset));
    }
    Ok(targets)
}

pub(super) fn emit_rewrite_finished(app: &AppHandle, session_id: &str) -> Result<(), String> {
    app.emit(
        "rewrite_finished",
        SessionEvent {
            session_id: session_id.to_string(),
        },
    )
    .map_err(|error| error.to_string())
}

pub(super) fn emit_rewrite_progress(
    app: &AppHandle,
    progress: RewriteProgressEvent<'_>,
) -> Result<(), String> {
    app.emit(
        "rewrite_progress",
        RewriteProgress {
            session_id: progress.session_id.to_string(),
            completed_units: progress.completed_units,
            in_flight: progress.in_flight,
            running_unit_ids: progress.running_unit_ids,
            total_units: progress.total_units,
            mode: progress.mode,
            running_state: progress.running_state,
            max_concurrency: progress.max_concurrency,
        },
    )
    .map_err(|error| error.to_string())
}

pub(super) fn auto_running_state(job: &JobControl) -> RunningState {
    if job.cancelled.load(Ordering::SeqCst) {
        return RunningState::Cancelled;
    }
    if job.paused.load(Ordering::SeqCst) {
        return RunningState::Paused;
    }
    RunningState::Running
}

#[cfg(test)]
#[path = "support_tests.rs"]
mod tests;
