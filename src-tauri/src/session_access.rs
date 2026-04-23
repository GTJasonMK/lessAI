use std::path::Path;

use chrono::{DateTime, Utc};
use log::warn;
use tauri::AppHandle;

use crate::{
    models::DocumentSession,
    observability::running_state_label,
    persist,
    result_flow::load_then,
    session_flow::{
        open_existing_or_clean_session_steps, run_session_steps, SessionLock, SessionStepConfig,
    },
    session_loader::{load_clean_session_from_disk, DiskCleanSessionLoadInput},
    session_refresh::refresh_session_from_disk,
    state::{ensure_no_active_job, load_job, AppState},
    storage,
};

pub(crate) enum SessionLoadSource<Guard = fn(&DocumentSession) -> Result<(), String>> {
    Stored,
    Refreshed { guard: Guard },
}

impl SessionLoadSource<fn(&DocumentSession) -> Result<(), String>> {
    pub(crate) fn stored() -> Self {
        Self::Stored
    }
}

impl<Guard> SessionLoadSource<Guard> {
    pub(crate) fn refreshed(guard: Guard) -> Self {
        Self::Refreshed { guard }
    }
}

pub(crate) struct CurrentSessionRequest<'a, Guard = fn(&DocumentSession) -> Result<(), String>> {
    pub(crate) app: &'a AppHandle,
    pub(crate) state: &'a AppState,
    pub(crate) session_id: &'a str,
    pub(crate) source: SessionLoadSource<Guard>,
    pub(crate) active_job_error: Option<&'a str>,
}

impl<'a> CurrentSessionRequest<'a, fn(&DocumentSession) -> Result<(), String>> {
    pub(crate) fn stored(app: &'a AppHandle, state: &'a AppState, session_id: &'a str) -> Self {
        Self::from_source(app, state, session_id, SessionLoadSource::stored())
    }
}

impl<'a, Guard> CurrentSessionRequest<'a, Guard> {
    fn from_source(
        app: &'a AppHandle,
        state: &'a AppState,
        session_id: &'a str,
        source: SessionLoadSource<Guard>,
    ) -> Self {
        Self {
            app,
            state,
            session_id,
            source,
            active_job_error: None,
        }
    }

    pub(crate) fn with_active_job_error(mut self, active_job_error: &'a str) -> Self {
        self.active_job_error = Some(active_job_error);
        self
    }

    pub(crate) fn guarded_refresh(
        app: &'a AppHandle,
        state: &'a AppState,
        session_id: &'a str,
        guard: Guard,
    ) -> Self {
        Self::from_source(app, state, session_id, SessionLoadSource::refreshed(guard))
    }
}

pub(crate) fn load_session_for_source<Guard, Load, Refresh>(
    source: SessionLoadSource<Guard>,
    load: Load,
    refresh: Refresh,
) -> Result<DocumentSession, String>
where
    Load: FnOnce() -> Result<DocumentSession, String>,
    Guard: FnOnce(&DocumentSession) -> Result<(), String>,
    Refresh: FnOnce(DocumentSession) -> Result<DocumentSession, String>,
{
    match source {
        SessionLoadSource::Stored => load(),
        SessionLoadSource::Refreshed { guard } => load_then(load, |session| {
            let refreshed = refresh(session)?;
            guard(&refreshed)?;
            Ok(refreshed)
        }),
    }
}

pub(crate) fn refresh_session_if_needed(
    app: &AppHandle,
    session: DocumentSession,
) -> Result<DocumentSession, String> {
    let refreshed = refresh_session_from_disk(app, &session)?;
    persist::maybe_save_and_return(refreshed.session, refreshed.changed, |session| {
        storage::save_session(app, session)
    })
}

pub(crate) fn open_session_for_path(
    app: &AppHandle,
    session_id: &str,
    canonical_path: &Path,
    document_path: String,
    created_at: DateTime<Utc>,
) -> Result<DocumentSession, String> {
    open_existing_or_clean_session_steps(
        || storage::load_session_optional(app, session_id),
        |session| storage::save_session(app, session),
        |session| refresh_session_if_needed(app, session),
        || {
            load_clean_session_from_disk(
                app,
                DiskCleanSessionLoadInput {
                    session_id: session_id.to_string(),
                    canonical_path,
                    document_path,
                    created_at,
                    reject_empty: true,
                },
            )
        },
        created_at,
    )
}

pub(crate) fn access_current_session<T, Guard, Run>(
    request: CurrentSessionRequest<'_, Guard>,
    run: Run,
) -> Result<T, String>
where
    Guard: FnOnce(&DocumentSession) -> Result<(), String>,
    Run: FnOnce(DocumentSession) -> Result<T, String>,
{
    let CurrentSessionRequest {
        app,
        state,
        session_id,
        source,
        active_job_error,
    } = request;
    if active_job_error.is_some() {
        ensure_no_active_job(state, session_id)?;
    }
    run_session_steps(
        || {
            let session = load_session_for_source(
                source,
                || storage::load_session(app, session_id),
                |session| refresh_session_if_needed(app, session),
            )?;
            repair_stale_active_session(app, state, session_id, session, active_job_error)
        },
        SessionStepConfig::locked(
            SessionLock::new(state, session_id),
            move |session: &DocumentSession| {
                ensure_loaded_session_is_idle(session_id, session, active_job_error)
            },
        ),
        run,
    )
}

pub(crate) fn mutate_current_session<T, Guard, Mutate>(
    request: CurrentSessionRequest<'_, Guard>,
    mutate: Mutate,
) -> Result<T, String>
where
    Guard: FnOnce(&DocumentSession) -> Result<(), String>,
    Mutate: FnOnce(&mut DocumentSession) -> Result<crate::session_edit::SessionMutation<T>, String>,
{
    let app = request.app;
    access_current_session(request, move |mut session| {
        let (value, should_save) = mutate(&mut session)?.into_parts();
        persist::maybe_save_and_return(value, should_save, |_| storage::save_session(app, &session))
    })
}

fn repair_stale_active_session(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
    session: DocumentSession,
    active_job_error: Option<&str>,
) -> Result<DocumentSession, String> {
    let previous_status = session.status;
    let session = repair_stale_active_session_steps(
        session,
        active_job_error,
        || Ok(load_job(state, session_id)?.is_some()),
        |session| storage::save_session(app, session),
        Utc::now(),
    )?;
    if previous_status != session.status {
        warn!(
            "repaired stale active session: source=stale_session_status session_id={session_id} previous_status={} current_status={}",
            running_state_label(previous_status),
            running_state_label(session.status),
        );
    }
    Ok(session)
}

fn repair_stale_active_session_steps<HasLiveJob, Save>(
    mut session: DocumentSession,
    active_job_error: Option<&str>,
    has_live_job: HasLiveJob,
    save: Save,
    now: DateTime<Utc>,
) -> Result<DocumentSession, String>
where
    HasLiveJob: FnOnce() -> Result<bool, String>,
    Save: FnOnce(&DocumentSession) -> Result<(), String>,
{
    if active_job_error.is_none() || !session.has_active_job() || has_live_job()? {
        return Ok(session);
    }

    session.downgrade_active_job_to_cancelled();
    crate::documents::hydrate_session_capabilities(&mut session);
    session.updated_at = now;
    save(&session)?;
    Ok(session)
}

fn ensure_loaded_session_is_idle(
    session_id: &str,
    session: &DocumentSession,
    active_job_error: Option<&str>,
) -> Result<(), String> {
    if let Some(active_job_error) = active_job_error {
        if session.has_active_job() {
            warn!(
                "rewrite gate blocked: source=session_status session_id={session_id} status={}",
                running_state_label(session.status),
            );
            return Err(active_job_error.to_string());
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "session_access_tests.rs"]
mod tests;
