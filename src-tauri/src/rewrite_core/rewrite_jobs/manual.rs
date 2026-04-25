use tauri::AppHandle;

use crate::{
    models::DocumentSession,
    rewrite_targets,
    session_access::{access_current_session, CurrentSessionRequest},
    state::AppState,
    storage,
};

use super::support::RewriteSessionAccess;
use super::{
    ensure_targets_available, process_loaded_rewrite_batch, resolve_available_rewrite_targets,
    rewrite_session_request,
};

pub(crate) async fn run_manual_rewrite(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
    target_rewrite_unit_ids: Option<Vec<String>>,
) -> Result<DocumentSession, String> {
    let session = access_current_session(
        rewrite_session_request(app, state, session_id, RewriteSessionAccess::ExternalEntry),
        Ok,
    )?;
    let settings = storage::load_settings(app)?;
    let targets = resolve_available_rewrite_targets(&session, target_rewrite_unit_ids)?;
    let next_batch = rewrite_targets::find_next_manual_batch(
        &session.rewrite_units,
        targets.target_unit_ids.as_ref(),
        settings.units_per_batch,
    );
    let next_batch =
        ensure_targets_available(next_batch, targets.has_target_subset, Vec::is_empty)?;

    process_loaded_rewrite_batch(app, state, &session.id, &session, &next_batch, false).await?;
    access_current_session(CurrentSessionRequest::stored(app, state, &session.id), Ok)
}
