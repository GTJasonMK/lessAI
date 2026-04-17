use tauri::AppHandle;

use crate::{
    models::DocumentSession,
    rewrite,
    rewrite_batch_commit::{
        batch_commit_mode, commit_rewrite_result, emit_rewrite_unit_completed_events,
    },
    rewrite_writeback::validate_candidate_batch_writeback,
    session_access::access_current_session,
    state::AppState,
    storage,
};

use super::{prepare_loaded_rewrite_batch, rewrite_session_request};
use super::support::RewriteSessionAccess;

pub(crate) async fn process_rewrite_unit(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
    rewrite_unit_id: &str,
    auto_approve: bool,
) -> Result<(), String> {
    process_rewrite_unit_batch(
        app,
        state,
        session_id,
        &[rewrite_unit_id.to_string()],
        auto_approve,
    )
    .await
}

async fn process_rewrite_unit_batch(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
    rewrite_unit_ids: &[String],
    auto_approve: bool,
) -> Result<(), String> {
    if rewrite_unit_ids.is_empty() {
        return Ok(());
    }

    let session = access_current_session(
        rewrite_session_request(app, state, session_id, RewriteSessionAccess::ExternalEntry),
        Ok,
    )?;
    process_loaded_rewrite_batch(
        app,
        state,
        session_id,
        &session,
        rewrite_unit_ids,
        auto_approve,
    )
    .await
}

pub(super) async fn process_loaded_rewrite_batch(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
    session: &DocumentSession,
    rewrite_unit_ids: &[String],
    auto_approve: bool,
) -> Result<(), String> {
    let settings = storage::load_settings(app)?;
    let prepared = prepare_loaded_rewrite_batch(session, rewrite_unit_ids)?;
    crate::rewrite_job_state::mark_units_running(app, state, session_id, rewrite_unit_ids)?;

    let completed_batch = commit_rewrite_result(
        app,
        state,
        session_id,
        &prepared.rewrite_unit_ids,
        rewrite::rewrite_batch(&settings, &prepared.batch_request).await,
        batch_commit_mode(auto_approve),
        validate_candidate_batch_writeback,
    )?;
    emit_rewrite_unit_completed_events(app, session_id, &completed_batch)?;
    Ok(())
}
