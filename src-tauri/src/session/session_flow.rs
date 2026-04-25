use chrono::{DateTime, Utc};

use crate::{
    models::DocumentSession,
    result_flow::load_then,
    state::{with_session_lock, AppState},
};

pub(crate) fn allow_session(_: &DocumentSession) -> Result<(), String> {
    Ok(())
}

pub(crate) struct SessionLock<'a> {
    state: &'a AppState,
    session_id: &'a str,
}

impl<'a> SessionLock<'a> {
    pub(crate) fn new(state: &'a AppState, session_id: &'a str) -> Self {
        Self { state, session_id }
    }
}

pub(crate) struct SessionStepConfig<'a, Guard> {
    lock: Option<SessionLock<'a>>,
    guard: Guard,
}

impl<'a, Guard> SessionStepConfig<'a, Guard> {
    #[cfg(test)]
    pub(crate) fn new(guard: Guard) -> Self {
        Self { lock: None, guard }
    }

    pub(crate) fn locked(lock: SessionLock<'a>, guard: Guard) -> Self {
        Self {
            lock: Some(lock),
            guard,
        }
    }
}

pub(crate) fn run_session_steps<'a, T, Load, Guard, Apply>(
    load: Load,
    config: SessionStepConfig<'a, Guard>,
    apply: Apply,
) -> Result<T, String>
where
    Load: FnOnce() -> Result<DocumentSession, String>,
    Guard: FnOnce(&DocumentSession) -> Result<(), String>,
    Apply: FnOnce(DocumentSession) -> Result<T, String>,
{
    let SessionStepConfig { lock, guard } = config;
    match lock {
        Some(lock) => with_session_lock(lock.state, lock.session_id, || {
            apply_session_steps(load, guard, apply)
        }),
        None => apply_session_steps(load, guard, apply),
    }
}

fn apply_session_steps<T, Load, Guard, Apply>(
    load: Load,
    guard: Guard,
    apply: Apply,
) -> Result<T, String>
where
    Load: FnOnce() -> Result<DocumentSession, String>,
    Guard: FnOnce(&DocumentSession) -> Result<(), String>,
    Apply: FnOnce(DocumentSession) -> Result<T, String>,
{
    load_then(load, |session| {
        guard(&session)?;
        apply(session)
    })
}

pub(crate) fn open_existing_or_clean_session_steps<LoadOptional, Save, Refresh, LoadClean>(
    load_optional: LoadOptional,
    save: Save,
    refresh: Refresh,
    load_clean: LoadClean,
    now: DateTime<Utc>,
) -> Result<DocumentSession, String>
where
    LoadOptional: FnOnce() -> Result<Option<DocumentSession>, String>,
    Save: Fn(&DocumentSession) -> Result<(), String>,
    Refresh: FnOnce(DocumentSession) -> Result<DocumentSession, String>,
    LoadClean: FnOnce() -> Result<DocumentSession, String>,
{
    if let Some(mut session) = load_optional()? {
        if session.downgrade_active_job_to_cancelled() {
            session.updated_at = now;
            save(&session)?;
        }
        return refresh(session);
    }

    let session = load_clean()?;
    save(&session)?;
    Ok(session)
}

#[cfg(test)]
#[path = "session_flow_tests.rs"]
mod tests;
