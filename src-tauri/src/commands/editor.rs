use serde::Deserialize;
use tauri::{AppHandle, State};

use crate::{
    documents::WritebackMode,
    editor_session::{
        ensure_editor_base_snapshot_matches_path, ACTIVE_EDITOR_SESSION_ERROR,
    },
    editor_writeback::{
        build_plain_text_editor_writeback, build_slot_editor_writeback, execute_editor_writeback,
        EditorWritebackPayload,
    },
    persist,
    models::{DocumentSession, DocumentSnapshot, EditorSlotEdit},
    session_access::{access_current_session, CurrentSessionRequest},
    session_loader::load_clean_session_from_existing,
    state::AppState,
    storage,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum EditorWritebackInput {
    Text { content: String },
    SlotEdits { edits: Vec<EditorSlotEdit> },
}

impl EditorWritebackInput {
    fn build(self, session: &DocumentSession) -> Result<EditorWritebackPayload, String> {
        match self {
            Self::Text { content } => build_plain_text_editor_writeback(session, &content),
            Self::SlotEdits { edits } => build_slot_editor_writeback(session, &edits),
        }
    }
}

#[tauri::command]
pub fn run_document_writeback(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    mode: WritebackMode,
    input: EditorWritebackInput,
    editor_base_snapshot: Option<DocumentSnapshot>,
) -> Result<DocumentSession, String> {
    access_current_session(
        CurrentSessionRequest::guarded_refresh(
            &app,
            state.inner(),
            &session_id,
            |session: &DocumentSession| {
                ensure_editor_base_snapshot_matches_path(
                    std::path::Path::new(&session.document_path),
                    editor_base_snapshot.as_ref(),
                )
            },
        )
        .with_active_job_error(ACTIVE_EDITOR_SESSION_ERROR),
        |session| {
            let payload = input.build(&session)?;
            execute_editor_writeback(&session, &payload, mode)?;
            finish_editor_writeback(&app, session, mode)
        },
    )
}

fn finish_editor_writeback(
    app: &AppHandle,
    session: DocumentSession,
    mode: WritebackMode,
) -> Result<DocumentSession, String> {
    match mode {
        WritebackMode::Validate => Ok(session),
        WritebackMode::Write => rebuild_saved_editor_session(app, session),
    }
}

fn rebuild_saved_editor_session(
    app: &AppHandle,
    session: DocumentSession,
) -> Result<DocumentSession, String> {
    let rebuilt = load_clean_session_from_existing(app, &session, session.created_at, false)?;
    persist::save_and_return(rebuilt, |rebuilt| storage::save_session(app, rebuilt))
}
