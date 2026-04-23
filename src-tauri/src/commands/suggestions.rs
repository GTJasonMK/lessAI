use tauri::{AppHandle, State};

use crate::{
    documents::{hydrated_session_clone, WritebackMode},
    models::{DocumentSession, RewriteUnitStatus, SuggestionDecision},
    rewrite_projection::{
        apply_suggestion_by_id, find_suggestion_index, SUGGESTION_NOT_FOUND_ERROR,
    },
    rewrite_writeback::execute_session_writeback,
    session_access::{mutate_current_session, CurrentSessionRequest},
    session_edit::SessionMutation,
    state::AppState,
};

#[tauri::command]
pub fn apply_suggestion(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    suggestion_id: String,
) -> Result<DocumentSession, String> {
    mutate_current_session(
        CurrentSessionRequest::guarded_refresh(
            &app,
            state.inner(),
            &session_id,
            crate::session_flow::allow_session,
        ),
        |session| {
            let now = chrono::Utc::now();
            apply_suggestion_by_id(session, &suggestion_id, now)?;
            execute_session_writeback(&session, WritebackMode::Validate)?;
            Ok(SessionMutation::save(session, now, hydrated_session_clone(session)))
        },
    )
}

#[tauri::command]
pub fn dismiss_suggestion(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    suggestion_id: String,
) -> Result<DocumentSession, String> {
    mutate_current_session(
        CurrentSessionRequest::stored(&app, state.inner(), &session_id),
        |session| {
            let now = chrono::Utc::now();
            let suggestion_index = find_suggestion_index(session, &suggestion_id)?;
            let suggestion = session
                .suggestions
                .get_mut(suggestion_index)
                .ok_or_else(|| SUGGESTION_NOT_FOUND_ERROR.to_string())?;

            suggestion.decision = SuggestionDecision::Dismissed;
            suggestion.updated_at = now;
            Ok(SessionMutation::save(session, now, hydrated_session_clone(session)))
        },
    )
}

#[tauri::command]
pub fn delete_suggestion(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    suggestion_id: String,
) -> Result<DocumentSession, String> {
    mutate_current_session(
        CurrentSessionRequest::stored(&app, state.inner(), &session_id),
        |session| {
            let now = chrono::Utc::now();
            let suggestion_index = find_suggestion_index(session, &suggestion_id)?;
            let removed_unit_id = session
                .suggestions
                .get(suggestion_index)
                .ok_or_else(|| SUGGESTION_NOT_FOUND_ERROR.to_string())?
                .rewrite_unit_id
                .clone();

            session.suggestions.retain(|item| item.id != suggestion_id);

            let still_has_any = session
                .suggestions
                .iter()
                .any(|item| item.rewrite_unit_id == removed_unit_id);

            if !still_has_any {
                if let Some(unit) = session
                    .rewrite_units
                    .iter_mut()
                    .find(|unit| unit.id == removed_unit_id)
                {
                    if unit.status == RewriteUnitStatus::Done {
                        unit.status = RewriteUnitStatus::Idle;
                    }
                }
            }

            Ok(SessionMutation::save(session, now, hydrated_session_clone(session)))
        },
    )
}
