use std::{
    fs,
    path::{Path, PathBuf},
};

use super::{
    ensure_editor_base_snapshot_matches_path, EDITOR_BASE_SNAPSHOT_EXPIRED_ERROR,
    EDITOR_BASE_SNAPSHOT_MISSING_ERROR,
};
use crate::{
    document_snapshot::capture_document_snapshot,
    test_support::{cleanup_dir, sample_clean_session, unique_test_dir},
};

fn sample_session(path: &PathBuf) -> crate::models::DocumentSession {
    sample_clean_session("session-1", &path.to_string_lossy(), "正文")
}

#[test]
fn rejects_missing_editor_base_snapshot() {
    let root = unique_test_dir("missing");
    fs::create_dir_all(&root).expect("create root");
    let target = root.join("sample.txt");
    fs::write(&target, "正文").expect("write source");

    let error = ensure_editor_base_snapshot_matches_path(&target, None)
        .expect_err("expected missing editor snapshot to be rejected");

    assert_eq!(error, EDITOR_BASE_SNAPSHOT_MISSING_ERROR);
    cleanup_dir(&root);
}

#[test]
fn rejects_stale_editor_base_snapshot() {
    let root = unique_test_dir("stale");
    fs::create_dir_all(&root).expect("create root");
    let target = root.join("sample.txt");
    fs::write(&target, "旧正文").expect("write source");
    let original_snapshot = capture_document_snapshot(&target).expect("capture original");
    fs::write(&target, "新正文").expect("simulate external change");

    let error = ensure_editor_base_snapshot_matches_path(&target, Some(&original_snapshot))
        .expect_err("expected stale editor snapshot to be rejected");

    assert_eq!(error, EDITOR_BASE_SNAPSHOT_EXPIRED_ERROR);
    cleanup_dir(&root);
}

#[test]
fn path_guard_uses_editor_base_snapshot_instead_of_session_snapshot() {
    let root = unique_test_dir("path-guard");
    fs::create_dir_all(&root).expect("create root");
    let target = root.join("sample.txt");
    fs::write(&target, "旧正文").expect("write original");
    let original_snapshot = capture_document_snapshot(&target).expect("capture original");
    fs::write(&target, "新正文").expect("simulate external change");

    let mut session = sample_session(&target);
    session.source_snapshot = Some(capture_document_snapshot(&target).expect("capture current"));

    let error = ensure_editor_base_snapshot_matches_path(
        Path::new(&session.document_path),
        Some(&original_snapshot),
    )
    .expect_err("expected wrapper to reject stale editor snapshot");

    assert_eq!(error, EDITOR_BASE_SNAPSHOT_EXPIRED_ERROR);
    cleanup_dir(&root);
}
