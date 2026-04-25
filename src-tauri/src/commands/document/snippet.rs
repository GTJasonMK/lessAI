use std::path::Path;

use tauri::{AppHandle, State};

use crate::{
    documents::{document_format, ensure_document_source_matches_session},
    editor_session::ensure_editor_base_snapshot_matches_path,
    editor_writeback::ensure_session_can_use_editor_writeback,
    models::{DocumentSession, DocumentSnapshot},
    rewrite,
    session_access::{access_current_session, CurrentSessionRequest},
    session_messages::ACTIVE_EDITOR_SESSION_ERROR,
    state::AppState,
    storage,
};

fn ensure_session_can_rewrite_snippet(session: &DocumentSession) -> Result<(), String> {
    ensure_session_can_use_editor_writeback(session)?;
    ensure_document_source_matches_session(
        Path::new(&session.document_path),
        session.source_snapshot.as_ref(),
    )
}

#[tauri::command]
pub async fn rewrite_selection(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    text: String,
    editor_base_snapshot: Option<DocumentSnapshot>,
) -> Result<String, String> {
    if text.trim().is_empty() {
        return Err("选区内容为空。".to_string());
    }

    let session = access_current_session(
        CurrentSessionRequest::guarded_refresh(
            &app,
            state.inner(),
            &session_id,
            |session: &DocumentSession| {
                ensure_editor_base_snapshot_matches_path(
                    Path::new(&session.document_path),
                    editor_base_snapshot.as_ref(),
                )
            },
        )
        .with_active_job_error(ACTIVE_EDITOR_SESSION_ERROR),
        |session| {
            ensure_session_can_rewrite_snippet(&session)?;
            Ok(session)
        },
    )?;

    let settings = storage::load_settings(&app)?;
    let format = document_format(Path::new(&session.document_path));
    rewrite::rewrite_selection_text(
        &settings,
        &text,
        format,
        session.rewrite_headings.unwrap_or(false),
    )
    .await
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::ensure_session_can_rewrite_snippet;
    use crate::{
        document_snapshot::capture_document_snapshot,
        models::{DocumentSession, SuggestionDecision},
        rewrite_unit::SlotUpdate,
        test_support::{
            cleanup_dir, editable_slot, rewrite_suggestion, rewrite_unit, sample_clean_session,
            write_temp_file,
        },
    };

    fn sample_session() -> DocumentSession {
        let mut session = sample_clean_session("session-1", "/tmp/example.docx", "正文");
        session.writeback_slots = vec![editable_slot("slot-0", 0, "正文")];
        session.rewrite_units = vec![rewrite_unit(
            "unit-0",
            0,
            &["slot-0"],
            "正文",
            crate::models::RewriteUnitStatus::Idle,
        )];
        session
    }

    #[test]
    fn rejects_snippet_rewrite_for_non_editor_safe_session() {
        let mut session = sample_session();
        session.capabilities.editor_writeback =
            crate::session_capability_models::CapabilityGate::blocked(
                "当前文档暂不支持进入编辑模式。",
            );
        crate::documents::hydrate_session_capabilities(&mut session);

        let error = ensure_session_can_rewrite_snippet(&session)
            .expect_err("expected snippet rewrite to be blocked");

        assert_eq!(error, "当前文档暂不支持进入编辑模式。");
    }

    #[test]
    fn rejects_snippet_rewrite_for_dirty_editor_session() {
        let (root, target) = write_temp_file("dirty-editor-session", "txt", "正文".as_bytes());

        let mut session = sample_session();
        session.document_path = target.to_string_lossy().to_string();
        session.source_text = "正文".to_string();
        session.source_snapshot =
            Some(capture_document_snapshot(&target).expect("capture initial snapshot"));
        session.suggestions.push(rewrite_suggestion(
            "s1",
            1,
            "unit-0",
            "正文",
            "改写正文",
            SuggestionDecision::Proposed,
            vec![SlotUpdate::new("slot-0", "改写正文")],
        ));
        crate::documents::hydrate_session_capabilities(&mut session);

        let error = ensure_session_can_rewrite_snippet(&session)
            .expect_err("expected dirty editor session to be blocked");

        assert!(error.contains("请先“覆写并清理记录”或“重置记录”后再编辑"));
        cleanup_dir(&root);
    }

    #[test]
    fn allows_snippet_rewrite_for_editor_safe_session() {
        let (root, target) = write_temp_file("source-match", "txt", "正文".as_bytes());

        let mut session = sample_session();
        session.document_path = target.to_string_lossy().to_string();
        session.source_text = "正文".to_string();
        session.source_snapshot =
            Some(capture_document_snapshot(&target).expect("capture initial snapshot"));

        ensure_session_can_rewrite_snippet(&session)
            .expect("expected snippet rewrite to be allowed");
        cleanup_dir(&root);
    }

    #[test]
    fn rejects_snippet_rewrite_when_source_changed_externally() {
        let (root, target) = write_temp_file("source-mismatch", "txt", "正文".as_bytes());

        let mut session = sample_session();
        session.document_path = target.to_string_lossy().to_string();
        session.source_text = "正文".to_string();
        session.source_snapshot =
            Some(capture_document_snapshot(&target).expect("capture initial snapshot"));

        fs::write(&target, "外部修改").expect("simulate external change");

        let error = ensure_session_can_rewrite_snippet(&session)
            .expect_err("expected snippet rewrite to be blocked");

        assert!(error.contains("原文件已在外部发生变化"));
        cleanup_dir(&root);
    }
}
