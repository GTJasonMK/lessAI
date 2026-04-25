use std::path::PathBuf;

use tauri::{AppHandle, State};

use crate::{
    atomic_write::write_bytes_atomically,
    documents::{normalize_text_against_source_layout, WritebackMode},
    rewrite_projection::build_applied_slot_projection,
    rewrite_unit::merged_text_from_slots,
    rewrite_writeback::execute_session_writeback,
    session_access::{access_current_session, CurrentSessionRequest},
    session_messages::ACTIVE_JOB_FINALIZE_ERROR,
    state::AppState,
    storage,
};

#[tauri::command]
pub fn export_document(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    path: String,
) -> Result<String, String> {
    let session = access_current_session(
        CurrentSessionRequest::stored(&app, state.inner(), &session_id),
        Ok,
    )?;
    let content = normalize_text_against_source_layout(
        &session.source_text,
        &merged_text_from_slots(&build_applied_slot_projection(&session)?),
    );
    let path_buf = PathBuf::from(&path);

    write_exported_text(&path_buf, &content)?;
    Ok(path)
}

#[tauri::command]
pub fn finalize_document(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
) -> Result<String, String> {
    access_current_session(
        CurrentSessionRequest::guarded_refresh(
            &app,
            state.inner(),
            &session_id,
            crate::session_flow::allow_session,
        )
        .with_active_job_error(ACTIVE_JOB_FINALIZE_ERROR),
        |session| {
            execute_session_writeback(&session, WritebackMode::Write)?;

            // 写回成功后再清理记录，避免“写失败但记录被删”的风险。
            storage::delete_session(&app, &session_id)?;

            Ok(session.document_path)
        },
    )
}

fn write_exported_text(path: &std::path::Path, content: &str) -> Result<(), String> {
    write_bytes_atomically(path, content.as_bytes())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::test_support::{cleanup_dir, unique_test_dir};

    #[test]
    fn write_exported_text_creates_parent_dirs_and_writes_content() {
        let root = unique_test_dir("write-exported-text");
        let target = root.join("nested").join("export.txt");

        super::write_exported_text(&target, "导出内容")
            .expect("expected exported text helper to create dirs and write");

        let stored = fs::read_to_string(&target).expect("read exported text");
        assert_eq!(stored, "导出内容");
        cleanup_dir(&root);
    }
}
