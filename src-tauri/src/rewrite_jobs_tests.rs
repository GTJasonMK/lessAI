use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
};

use chrono::Utc;
use uuid::Uuid;
use zip::{write::FileOptions, ZipWriter};

use super::{validate_candidate_writeback, validate_session_writeback};
use crate::{
    document_snapshot::capture_document_snapshot,
    models::{
        ChunkStatus, ChunkTask, DocumentSession, EditSuggestion, RunningState, SuggestionDecision,
    },
};

fn unique_test_dir(name: &str) -> PathBuf {
    env::temp_dir().join(format!("lessai-rewrite-jobs-{name}-{}", Uuid::new_v4()))
}

fn cleanup_dir(path: &Path) {
    let _ = fs::remove_dir_all(path);
}

fn write_temp_file(name: &str, ext: &str, contents: &[u8]) -> (PathBuf, PathBuf) {
    let root = unique_test_dir(name);
    fs::create_dir_all(&root).expect("create root");
    let target = root.join(format!("sample.{ext}"));
    fs::write(&target, contents).expect("write temp file");
    (root, target)
}

fn build_minimal_docx(document_xml: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let cursor = std::io::Cursor::new(&mut out);
    let mut zip = ZipWriter::new(cursor);
    let options = FileOptions::<()>::default();
    zip.start_file("word/document.xml", options)
        .expect("start document.xml");
    zip.write_all(document_xml.as_bytes())
        .expect("write document.xml");
    zip.finish().expect("finish docx");
    out
}

fn sample_docx_session(path: &Path) -> DocumentSession {
    let now = Utc::now();
    DocumentSession {
        id: "session-1".to_string(),
        title: "示例".to_string(),
        document_path: path.to_string_lossy().to_string(),
        source_text: "正文".to_string(),
        source_snapshot: Some(capture_document_snapshot(path).expect("capture snapshot")),
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
fn validate_candidate_writeback_rejects_docx_candidate_that_changes_paragraph_boundaries() {
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>正文</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(document_xml);
    let (root, target) = write_temp_file("candidate-validate-fail", "docx", &bytes);
    let session = sample_docx_session(&target);

    let error = validate_candidate_writeback(&session, 0, "正文\n\n新增段")
        .expect_err("expected candidate validation failure");

    assert!(
        error.contains("段落")
            || error.contains("空段落边界")
            || error.contains("写回内容与原 docx 结构不一致")
    );
    cleanup_dir(&root);
}

#[test]
fn validate_session_writeback_rejects_unwritable_docx_applied_suggestion() {
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>正文</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(document_xml);
    let (root, target) = write_temp_file("session-validate-fail", "docx", &bytes);
    let mut session = sample_docx_session(&target);
    let now = Utc::now();
    session.suggestions.push(EditSuggestion {
        id: "suggestion-1".to_string(),
        sequence: 1,
        chunk_index: 0,
        before_text: "正文".to_string(),
        after_text: "正文\n\n新增段".to_string(),
        diff_spans: Vec::new(),
        decision: SuggestionDecision::Applied,
        created_at: now,
        updated_at: now,
    });

    let error =
        validate_session_writeback(&session).expect_err("expected applied validation failure");

    assert!(
        error.contains("段落")
            || error.contains("空段落边界")
            || error.contains("写回内容与原 docx 结构不一致")
    );
    cleanup_dir(&root);
}
