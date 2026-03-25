use chrono::Utc;
use tauri::{AppHandle, State};

use crate::{
    models::{ChunkStatus, DocumentSession, RunningState, SuggestionDecision},
    state::{with_session_lock, AppState},
    storage,
};

#[tauri::command]
pub fn apply_suggestion(
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
pub fn dismiss_suggestion(
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
pub fn delete_suggestion(
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
