use chrono::Utc;

use crate::{
    document_snapshot::capture_document_snapshot,
    documents::{OwnedDocumentWriteback, WritebackMode},
    rewrite_unit::{RewriteUnitResponse, SlotUpdate},
    test_support::{
        build_minimal_docx, cleanup_dir, editable_slot, locked_slot, rewrite_suggestion,
        rewrite_unit, write_temp_file,
    },
};

fn sample_plain_text_session(path: &std::path::Path) -> crate::models::DocumentSession {
    let now = Utc::now();
    crate::models::DocumentSession {
        id: "session-text".to_string(),
        title: "示例".to_string(),
        document_path: path.to_string_lossy().to_string(),
        source_text: "原文\r\n下一行\r\n".to_string(),
        source_snapshot: Some(capture_document_snapshot(path).expect("capture snapshot")),
        normalized_text: "原文\r\n下一行\r\n".to_string(),
        write_back_supported: true,
        write_back_block_reason: None,
        plain_text_editor_safe: true,
        plain_text_editor_block_reason: None,
        segmentation_preset: Some(crate::models::SegmentationPreset::Paragraph),
        rewrite_headings: Some(false),
        writeback_slots: vec![editable_slot("slot-0", 0, "原文\r\n下一行\r\n")],
        rewrite_units: vec![rewrite_unit(
            "unit-0",
            0,
            &["slot-0"],
            "原文\r\n下一行\r\n",
            crate::models::RewriteUnitStatus::Idle,
        )],
        suggestions: vec![rewrite_suggestion(
            "suggestion-1",
            1,
            "unit-0",
            "原文\r\n下一行\r\n",
            "新文\n下一行  \n",
            crate::models::SuggestionDecision::Applied,
            vec![SlotUpdate::new("slot-0", "新文\n下一行  \n")],
        )],
        next_suggestion_sequence: 2,
        status: crate::models::RunningState::Idle,
        created_at: now,
        updated_at: now,
    }
}

fn sample_docx_session(path: &std::path::Path) -> crate::models::DocumentSession {
    let now = Utc::now();
    crate::models::DocumentSession {
        id: "session-docx".to_string(),
        title: "示例".to_string(),
        document_path: path.to_string_lossy().to_string(),
        source_text: "前文[公式]后文".to_string(),
        source_snapshot: Some(capture_document_snapshot(path).expect("capture snapshot")),
        normalized_text: "前文[公式]后文".to_string(),
        write_back_supported: true,
        write_back_block_reason: None,
        plain_text_editor_safe: false,
        plain_text_editor_block_reason: Some("docx 仅支持槽位编辑".to_string()),
        segmentation_preset: Some(crate::models::SegmentationPreset::Paragraph),
        rewrite_headings: Some(false),
        writeback_slots: vec![
            editable_slot("slot-0", 0, "前文"),
            locked_slot("slot-1", 1, "[公式]"),
            editable_slot("slot-2", 2, "后文"),
        ],
        rewrite_units: vec![rewrite_unit(
            "unit-0",
            0,
            &["slot-0", "slot-1", "slot-2"],
            "前文[公式]后文",
            crate::models::RewriteUnitStatus::Idle,
        )],
        suggestions: Vec::new(),
        next_suggestion_sequence: 1,
        status: crate::models::RunningState::Idle,
        created_at: now,
        updated_at: now,
    }
}

#[test]
fn build_session_writeback_plan_returns_plain_text_output() {
    let (root, target) = write_temp_file("session-plan", "txt", "原文\r\n下一行\r\n".as_bytes());
    let session = sample_plain_text_session(&target);

    match super::build_session_writeback_plan(&session) {
        Ok(OwnedDocumentWriteback::Text(text)) => assert_eq!(text, "新文\n下一行  \n"),
        Ok(OwnedDocumentWriteback::Slots(_)) => panic!("expected plain-text output"),
        Err(error) => panic!("unexpected error: {error}"),
    }

    cleanup_dir(&root);
}

#[test]
fn build_session_writeback_plan_returns_updated_slots_for_docx() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:t>前文</w:t></w:r>
      <w:r><w:t>[公式]</w:t></w:r>
      <w:r><w:t>后文</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let (root, target) = write_temp_file("docx-plan", "docx", &bytes);
    let mut session = sample_docx_session(&target);
    session.suggestions.push(rewrite_suggestion(
        "suggestion-1",
        1,
        "unit-0",
        "前文[公式]后文",
        "新前文[公式]新后文",
        crate::models::SuggestionDecision::Applied,
        vec![
            SlotUpdate::new("slot-0", "新前文"),
            SlotUpdate::new("slot-2", "新后文"),
        ],
    ));

    match super::build_session_writeback_plan(&session) {
        Ok(OwnedDocumentWriteback::Slots(slots)) => {
            assert_eq!(slots[0].text, "新前文");
            assert_eq!(slots[1].text, "[公式]");
            assert_eq!(slots[2].text, "新后文");
        }
        Ok(OwnedDocumentWriteback::Text(_)) => panic!("expected docx slots output"),
        Err(error) => panic!("unexpected error: {error}"),
    }

    cleanup_dir(&root);
}

#[test]
fn validate_candidate_batch_writeback_rejects_locked_slot_update() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:t>前文</w:t></w:r>
      <w:r><w:t>[公式]</w:t></w:r>
      <w:r><w:t>后文</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let (root, target) = write_temp_file("candidate-locked", "docx", &bytes);
    let mut session = sample_docx_session(&target);
    session.writeback_slots[1].role = crate::rewrite_unit::WritebackSlotRole::InlineObject;

    let error = super::validate_candidate_batch_writeback(
        &session,
        &[RewriteUnitResponse {
            rewrite_unit_id: "unit-0".to_string(),
            updates: vec![SlotUpdate::new("slot-1", "改坏公式")],
        }],
    )
    .expect_err("locked slot update should fail");

    assert!(error.contains("locked slot"));
    cleanup_dir(&root);
}

#[test]
fn execute_session_writeback_returns_block_error_before_loading_source() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:t>前文</w:t></w:r>
      <w:r><w:t>[公式]</w:t></w:r>
      <w:r><w:t>后文</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let (root, target) = write_temp_file("blocked-session", "docx", &bytes);
    let mut session = sample_docx_session(&target);
    session.write_back_supported = false;
    session.write_back_block_reason = Some("blocked".to_string());
    session.suggestions.push(rewrite_suggestion(
        "suggestion-1",
        1,
        "unit-0",
        "前文[公式]后文",
        "改写后",
        crate::models::SuggestionDecision::Applied,
        vec![SlotUpdate::new("slot-0", "改写后")],
    ));

    let error = super::execute_session_writeback(&session, WritebackMode::Validate)
        .expect_err("blocked session should short-circuit");

    assert_eq!(error, "blocked");
    cleanup_dir(&root);
}
