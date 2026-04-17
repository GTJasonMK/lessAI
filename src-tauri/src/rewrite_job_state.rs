use tauri::AppHandle;

use crate::{
    models::{RewriteUnitStatus, DocumentSession, RunningState},
    rewrite_permissions::REWRITE_UNIT_NOT_FOUND_ERROR,
    session_access::{mutate_current_session, CurrentSessionRequest},
    session_edit::SessionMutation,
    state::AppState,
};

pub(crate) fn clear_running_units(session: &mut DocumentSession) -> bool {
    update_running_units(session, RewriteUnitStatus::Idle, None)
}

pub(crate) fn fail_running_units(session: &mut DocumentSession, error: &str) -> bool {
    update_running_units(session, RewriteUnitStatus::Failed, Some(error))
}

fn update_running_units(
    session: &mut DocumentSession,
    status: RewriteUnitStatus,
    error_message: Option<&str>,
) -> bool {
    let mut touched = false;
    let error_message = error_message.map(str::to_string);
    for unit in &mut session.rewrite_units {
        if unit.status != RewriteUnitStatus::Running {
            continue;
        }
        unit.status = status;
        unit.error_message = error_message.clone();
        touched = true;
    }
    touched
}

pub(crate) fn update_target_units(
    session: &mut DocumentSession,
    rewrite_unit_ids: &[String],
    status: RewriteUnitStatus,
    error_message: Option<&str>,
) -> Result<(), String> {
    for rewrite_unit_id in rewrite_unit_ids {
        if !session.rewrite_units.iter().any(|unit| &unit.id == rewrite_unit_id) {
            return Err(REWRITE_UNIT_NOT_FOUND_ERROR.to_string());
        }
    }

    let error_message = error_message.map(str::to_string);
    for rewrite_unit_id in rewrite_unit_ids {
        let unit = session
            .rewrite_units
            .iter_mut()
            .find(|unit| &unit.id == rewrite_unit_id)
            .ok_or_else(|| REWRITE_UNIT_NOT_FOUND_ERROR.to_string())?;
        unit.status = status;
        unit.error_message = error_message.clone();
    }
    Ok(())
}

pub(crate) fn set_units_running_status(
    session: &mut DocumentSession,
    rewrite_unit_ids: &[String],
) -> Result<(), String> {
    update_target_units(session, rewrite_unit_ids, RewriteUnitStatus::Running, None)?;
    if session.status != RunningState::Paused {
        session.status = RunningState::Running;
    }
    Ok(())
}

pub(crate) fn fail_target_units_and_reset_other_running(
    session: &mut DocumentSession,
    rewrite_unit_ids: &[String],
    error: &str,
) -> Result<(), String> {
    update_target_units(
        session,
        rewrite_unit_ids,
        RewriteUnitStatus::Failed,
        Some(error),
    )?;
    session.status = RunningState::Failed;
    clear_running_units(session);
    Ok(())
}

pub(crate) fn compute_session_state(session: &DocumentSession) -> RunningState {
    if session
        .rewrite_units
        .iter()
        .any(|unit| unit.status == RewriteUnitStatus::Failed)
    {
        return RunningState::Failed;
    }
    if session
        .rewrite_units
        .iter()
        .all(|unit| unit.status == RewriteUnitStatus::Done)
    {
        return RunningState::Completed;
    }
    RunningState::Idle
}

pub(crate) fn set_session_cancelled(session: &mut DocumentSession) {
    session.status = RunningState::Cancelled;
    clear_running_units(session);
}

pub(crate) fn set_session_paused(session: &mut DocumentSession) {
    session.status = RunningState::Paused;
}

pub(crate) fn set_session_running(session: &mut DocumentSession) {
    session.status = RunningState::Running;
}

fn mark_session_transition(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
    transition: fn(&mut DocumentSession),
) -> Result<DocumentSession, String> {
    mutate_stored_session_now(app, state, session_id, |session| {
        transition(session);
        Ok(session.clone())
    })
}

fn mutate_stored_session_now<T>(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
    mutate: impl FnOnce(&mut DocumentSession) -> Result<T, String>,
) -> Result<T, String> {
    mutate_current_session(
        CurrentSessionRequest::stored(app, state, session_id),
        |session| {
            let now = chrono::Utc::now();
            let value = mutate(session)?;
            Ok(SessionMutation::save(session, now, value))
        },
    )
}

pub(crate) fn mark_session_cancelled(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
) -> Result<DocumentSession, String> {
    mark_session_transition(app, state, session_id, set_session_cancelled)
}

pub(crate) fn mark_session_paused(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
) -> Result<DocumentSession, String> {
    mark_session_transition(app, state, session_id, set_session_paused)
}

pub(crate) fn mark_session_running(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
) -> Result<DocumentSession, String> {
    mark_session_transition(app, state, session_id, set_session_running)
}

pub(crate) fn finalize_auto_session(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
) -> Result<RunningState, String> {
    mutate_stored_session_now(app, state, session_id, |session| {
            session.status = compute_session_state(session);
            Ok(session.status)
        })
}

pub(crate) fn mark_units_running(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
    rewrite_unit_ids: &[String],
) -> Result<(), String> {
    mutate_stored_session_now(app, state, session_id, |session| {
            set_units_running_status(session, rewrite_unit_ids)?;
            Ok(())
        })
}

pub(crate) fn mark_session_failed(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
    error: String,
) -> Result<(), String> {
    mutate_stored_session_now(app, state, session_id, |session| {
            session.status = RunningState::Failed;
            fail_running_units(session, &error);
            Ok(())
        })
}

pub(crate) fn mark_auto_batch_failed(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
    rewrite_unit_ids: &[String],
    error: String,
) -> Result<(), String> {
    mutate_stored_session_now(app, state, session_id, |session| {
            fail_target_units_and_reset_other_running(session, rewrite_unit_ids, &error)?;
            Ok(())
        })
}
