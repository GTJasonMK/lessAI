use std::path::Path;

use tauri::{AppHandle, State};

use crate::{
    documents::{document_format, ensure_document_source_matches_session},
    models::DocumentSession,
    rewrite,
    session_repair::load_session_with_snapshot_repairs,
    state::AppState,
    storage,
};

fn ensure_session_can_rewrite_snippet(session: &DocumentSession) -> Result<(), String> {
    if !session.plain_text_editor_safe {
        return Err(session
            .plain_text_editor_block_reason
            .clone()
            .unwrap_or_else(|| "当前文档暂不支持进入编辑模式。".to_string()));
    }
    ensure_document_source_matches_session(
        Path::new(&session.document_path),
        &session.source_text,
        session.source_snapshot.as_ref(),
    )
}

#[tauri::command]
pub async fn rewrite_snippet(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    text: String,
) -> Result<String, String> {
    if text.trim().is_empty() {
        return Err("选区内容为空。".to_string());
    }

    {
        // 避免与后台 job 竞争使用同一 session（尤其是自动批处理还在跑的时候）。
        let jobs = state
            .jobs
            .lock()
            .map_err(|_| "任务状态锁已损坏。".to_string())?;
        if jobs.contains_key(&session_id) {
            return Err("后台任务仍在运行或正在退出，请稍后再试。".to_string());
        }
    }

    let session = load_session_with_snapshot_repairs(&app, state.inner(), &session_id)?;
    ensure_session_can_rewrite_snippet(&session)?;

    let settings = storage::load_settings(&app)?;
    let format = document_format(Path::new(&session.document_path));
    rewrite::rewrite_chunk(&settings, &text, format).await
}

#[cfg(test)]
mod tests {
    use std::{
        env, fs,
        path::{Path, PathBuf},
    };

    use chrono::Utc;
    use uuid::Uuid;

    use super::ensure_session_can_rewrite_snippet;
    use crate::{
        document_snapshot::capture_document_snapshot,
        models::{ChunkStatus, ChunkTask, DocumentSession, RunningState},
    };

    fn unique_test_dir(name: &str) -> PathBuf {
        env::temp_dir().join(format!("lessai-snippet-{name}-{}", Uuid::new_v4()))
    }

    fn cleanup_dir(path: &Path) {
        let _ = fs::remove_dir_all(path);
    }

    fn sample_session() -> DocumentSession {
        let now = Utc::now();
        DocumentSession {
            id: "session-1".to_string(),
            title: "示例".to_string(),
            document_path: "/tmp/example.docx".to_string(),
            source_text: "正文".to_string(),
            source_snapshot: None,
            normalized_text: "正文".to_string(),
            write_back_supported: true,
            write_back_block_reason: None,
            plain_text_editor_safe: true,
            plain_text_editor_block_reason: None,
            chunk_preset: Some(crate::models::ChunkPreset::Paragraph),
            rewrite_headings: Some(false),
            chunks: vec![ChunkTask {
                index: 0,
                source_text: "正文".to_string(),
                separator_after: String::new(),
                skip_rewrite: false,
                presentation: None,
                status: ChunkStatus::Idle,
                error_message: None,
            }],
            suggestions: Vec::new(),
            next_suggestion_sequence: 1,
            status: RunningState::Idle,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn rejects_snippet_rewrite_for_non_editor_safe_session() {
        let mut session = sample_session();
        session.plain_text_editor_safe = false;
        session.plain_text_editor_block_reason = Some("当前文档暂不支持进入编辑模式。".to_string());

        let error = ensure_session_can_rewrite_snippet(&session)
            .expect_err("expected snippet rewrite to be blocked");

        assert_eq!(error, "当前文档暂不支持进入编辑模式。");
    }

    #[test]
    fn allows_snippet_rewrite_for_editor_safe_session() {
        let root = unique_test_dir("source-match");
        fs::create_dir_all(&root).expect("create root");
        let target = root.join("sample.txt");
        fs::write(&target, "正文").expect("write source file");

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
        let root = unique_test_dir("source-mismatch");
        fs::create_dir_all(&root).expect("create root");
        let target = root.join("sample.txt");
        fs::write(&target, "正文").expect("write source file");

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
