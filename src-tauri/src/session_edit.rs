use chrono::{DateTime, Utc};

use crate::models::DocumentSession;

pub(crate) enum SessionMutation<T> {
    Save(T),
    SkipSave(T),
}

impl<T> SessionMutation<T> {
    pub(crate) fn save(session: &mut DocumentSession, updated_at: DateTime<Utc>, value: T) -> Self {
        crate::documents::hydrate_session_capabilities(session);
        session.updated_at = updated_at;
        Self::Save(value)
    }

    pub(crate) fn into_parts(self) -> (T, bool) {
        match self {
            Self::Save(value) => (value, true),
            Self::SkipSave(value) => (value, false),
        }
    }
}

#[cfg(test)]
#[path = "session_edit_tests.rs"]
mod tests;
