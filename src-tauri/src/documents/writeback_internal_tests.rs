use std::fs;

use crate::test_support::{cleanup_dir, write_temp_file};

fn plain_text_source(text: &str) -> super::VerifiedWritebackSource {
    let template = crate::adapters::plain_text::PlainTextAdapter::build_template(text);
    super::VerifiedWritebackSource::Textual(template)
}

#[test]
fn finish_document_writeback_skips_disk_write_in_validate_mode() {
    let (root, target) = write_temp_file("document-writeback-validate", "txt", b"original");

    super::finish_document_writeback(&target, b"updated", super::WritebackMode::Validate)
        .expect("expected validate mode to skip disk write");

    let stored = fs::read(&target).expect("read untouched file");
    assert_eq!(stored, b"original");
    cleanup_dir(&root);
}

#[test]
fn finish_document_writeback_persists_bytes_in_write_mode() {
    let (root, target) = write_temp_file("document-writeback-write", "txt", b"original");

    super::finish_document_writeback(&target, b"updated", super::WritebackMode::Write)
        .expect("expected write mode to persist bytes");

    let stored = fs::read(&target).expect("read updated file");
    assert_eq!(stored, b"updated");
    cleanup_dir(&root);
}

#[test]
fn build_text_writeback_bytes_returns_plain_text_bytes_for_plain_text_source() {
    let bytes =
        super::build_text_writeback_bytes(&plain_text_source("原始内容"), "原始内容", "新的内容")
            .expect("expected plain text writeback bytes");

    assert_eq!(bytes, "新的内容".as_bytes());
}

#[test]
fn build_slot_writeback_bytes_rejects_reordered_plain_text_slots() {
    let template =
        crate::adapters::plain_text::PlainTextAdapter::build_template("第一段\n\n第二段");
    let built = crate::textual_template::slots::build_slots(&template);
    let mut slots = built.slots.clone();
    slots.swap(0, 1);

    let error = super::build_slot_writeback_bytes(
        &plain_text_source("第一段\n\n第二段"),
        super::DocumentWritebackContext::new("第一段\n\n第二段", None).with_structure_signatures(
            Some(&template.template_signature),
            Some(&built.slot_structure_signature),
            false,
        ),
        &slots,
    )
    .expect_err("expected reordered plain text slots to be rejected");

    assert!(error.contains("结构"));
}

#[test]
fn writeback_mode_deserializes_from_command_payload() {
    let validate: super::WritebackMode =
        serde_json::from_str("\"validate\"").expect("deserialize validate mode");
    let write: super::WritebackMode =
        serde_json::from_str("\"write\"").expect("deserialize write mode");

    assert_eq!(validate, super::WritebackMode::Validate);
    assert_eq!(write, super::WritebackMode::Write);
}
