use chrono::Utc;

use crate::{
    adapters::plain_text::PlainTextAdapter,
    document_snapshot::capture_document_snapshot,
    documents::{load_document_source, OwnedDocumentWriteback, WritebackMode},
    rewrite_unit::{RewriteUnitResponse, SlotUpdate},
    test_support::{
        build_minimal_docx, build_minimal_pdf, cleanup_dir, editable_slot, locked_slot,
        rewrite_suggestion, rewrite_unit, write_temp_file,
    },
};

fn sample_plain_text_session(path: &std::path::Path) -> crate::models::DocumentSession {
    let now = Utc::now();
    let template = PlainTextAdapter::build_template("原文\r\n下一行\r\n");
    let built = crate::textual_template::slots::build_slots(&template);
    let rewrite_units = crate::rewrite_unit::build_rewrite_units(
        &built.slots,
        crate::models::SegmentationPreset::Paragraph,
    );
    let slot_id = built.slots[0].id.clone();

    let mut session = crate::models::DocumentSession {
        id: "session-text".to_string(),
        title: "示例".to_string(),
        document_path: path.to_string_lossy().to_string(),
        source_text: "原文\r\n下一行\r\n".to_string(),
        source_snapshot: Some(capture_document_snapshot(path).expect("capture snapshot")),
        template_kind: Some(template.kind.clone()),
        template_signature: Some(template.template_signature.clone()),
        slot_structure_signature: Some(built.slot_structure_signature.clone()),
        template_snapshot: Some(template),
        normalized_text: "原文\r\n下一行\r\n".to_string(),
        capabilities: crate::session_capability_models::DocumentSessionCapabilities {
            source_writeback: crate::session_capability_models::CapabilityGate::allowed(),
            editor_writeback: crate::session_capability_models::CapabilityGate::allowed(),
            ..Default::default()
        },
        segmentation_preset: Some(crate::models::SegmentationPreset::Paragraph),
        rewrite_headings: Some(false),
        writeback_slots: built.slots,
        rewrite_units,
        suggestions: vec![rewrite_suggestion(
            "suggestion-1",
            1,
            "unit-0",
            "原文\r\n下一行\r\n",
            "新文\n下一行  ",
            crate::models::SuggestionDecision::Applied,
            vec![SlotUpdate::new(&slot_id, "新文\n下一行  ")],
        )],
        next_suggestion_sequence: 2,
        status: crate::models::RunningState::Idle,
        created_at: now,
        updated_at: now,
    };
    crate::documents::hydrate_session_capabilities(&mut session);
    session
}

fn sample_docx_session(path: &std::path::Path) -> crate::models::DocumentSession {
    let now = Utc::now();
    let mut session = crate::models::DocumentSession {
        id: "session-docx".to_string(),
        title: "示例".to_string(),
        document_path: path.to_string_lossy().to_string(),
        source_text: "前文[公式]后文".to_string(),
        source_snapshot: Some(capture_document_snapshot(path).expect("capture snapshot")),
        template_kind: None,
        template_signature: None,
        slot_structure_signature: None,
        template_snapshot: None,
        normalized_text: "前文[公式]后文".to_string(),
        capabilities: crate::session_capability_models::DocumentSessionCapabilities {
            source_writeback: crate::session_capability_models::CapabilityGate::allowed(),
            editor_writeback: crate::session_capability_models::CapabilityGate::blocked(
                "docx 仅支持槽位编辑",
            ),
            ..Default::default()
        },
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
    };
    crate::documents::hydrate_session_capabilities(&mut session);
    session
}

fn sample_pdf_session(path: &std::path::Path) -> crate::models::DocumentSession {
    let now = Utc::now();
    let loaded = load_document_source(path, false).expect("load pdf");
    let rewrite_units = crate::rewrite_unit::build_rewrite_units(
        &loaded.writeback_slots,
        crate::models::SegmentationPreset::Paragraph,
    );
    let slot_id = loaded
        .writeback_slots
        .iter()
        .find(|slot| slot.editable)
        .expect("editable pdf slot")
        .id
        .clone();

    let mut session = crate::models::DocumentSession {
        id: "session-pdf".to_string(),
        title: "示例".to_string(),
        document_path: path.to_string_lossy().to_string(),
        source_text: loaded.source_text.clone(),
        source_snapshot: Some(capture_document_snapshot(path).expect("capture snapshot")),
        template_kind: loaded.template_kind.clone(),
        template_signature: loaded.template_signature.clone(),
        slot_structure_signature: loaded.slot_structure_signature.clone(),
        template_snapshot: loaded.template_snapshot.clone(),
        normalized_text: loaded.source_text.clone(),
        capabilities: crate::session_capability_models::DocumentSessionCapabilities {
            source_writeback: loaded.capability_policy.source_writeback.clone(),
            editor_writeback: loaded.capability_policy.editor_writeback.clone(),
            ..Default::default()
        },
        segmentation_preset: Some(crate::models::SegmentationPreset::Paragraph),
        rewrite_headings: Some(false),
        writeback_slots: loaded.writeback_slots,
        rewrite_units,
        suggestions: vec![rewrite_suggestion(
            "suggestion-1",
            1,
            "unit-0",
            "Alpha line\n",
            "Alpha revised\n",
            crate::models::SuggestionDecision::Applied,
            vec![SlotUpdate::new(&slot_id, "Alpha revised")],
        )],
        next_suggestion_sequence: 2,
        status: crate::models::RunningState::Idle,
        created_at: now,
        updated_at: now,
    };
    crate::documents::hydrate_session_capabilities(&mut session);
    session
}

#[test]
fn build_session_writeback_plan_returns_plain_text_output() {
    let (root, target) = write_temp_file("session-plan", "txt", "原文\r\n下一行\r\n".as_bytes());
    let session = sample_plain_text_session(&target);

    match super::build_session_writeback_plan(&session) {
        Ok(OwnedDocumentWriteback::Slots(slots)) => {
            assert_eq!(slots.len(), 1);
            assert_eq!(slots[0].text, "新文\n下一行  ");
            assert_eq!(slots[0].separator_after, "\r\n");
        }
        Ok(OwnedDocumentWriteback::Text(_)) => panic!("expected slot output"),
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
    session.capabilities.source_writeback =
        crate::session_capability_models::CapabilityGate::blocked("blocked");
    session.suggestions.push(rewrite_suggestion(
        "suggestion-1",
        1,
        "unit-0",
        "前文[公式]后文",
        "改写后",
        crate::models::SuggestionDecision::Applied,
        vec![SlotUpdate::new("slot-0", "改写后")],
    ));
    crate::documents::hydrate_session_capabilities(&mut session);

    let error = super::execute_session_writeback(&session, WritebackMode::Validate)
        .expect_err("blocked session should short-circuit");

    assert_eq!(error, "blocked");
    cleanup_dir(&root);
}

#[test]
fn execute_session_writeback_validates_plain_text_slot_projection() {
    let (root, target) = write_temp_file(
        "plain-text-slot-writeback",
        "txt",
        "原文\r\n下一行\r\n".as_bytes(),
    );
    let session = sample_plain_text_session(&target);

    super::execute_session_writeback(&session, WritebackMode::Validate)
        .expect("plain-text slot projection should validate");

    cleanup_dir(&root);
}

#[test]
fn build_session_writeback_plan_returns_slots_for_safe_pdf() {
    let bytes = build_minimal_pdf(&["Alpha line", "Beta line"]);
    let (root, target) = write_temp_file("pdf-plan-safe", "pdf", &bytes);
    let session = sample_pdf_session(&target);

    match super::build_session_writeback_plan(&session) {
        Ok(OwnedDocumentWriteback::Slots(slots)) => {
            assert_eq!(slots[0].text, "Alpha revised");
        }
        Ok(OwnedDocumentWriteback::Text(_)) => panic!("expected safe pdf slots output"),
        Err(error) => panic!("unexpected error: {error}"),
    }

    cleanup_dir(&root);
}

#[test]
fn build_session_writeback_plan_keeps_slot_shape_for_unsafe_pdf() {
    let bytes = build_minimal_pdf(&["Repeat", "Repeat"]);
    let (root, target) = write_temp_file("pdf-plan-unsafe", "pdf", &bytes);
    let mut session = sample_pdf_session(&target);
    session.suggestions[0].slot_updates =
        vec![SlotUpdate::new(&session.writeback_slots[0].id, "Rewritten")];
    session.suggestions[0].after_text = "Rewritten\n".to_string();

    match super::build_session_writeback_plan(&session) {
        Ok(OwnedDocumentWriteback::Slots(slots)) => {
            assert_eq!(slots[0].text, "Rewritten");
        }
        Ok(OwnedDocumentWriteback::Text(_)) => panic!("expected unsafe pdf slots output"),
        Err(error) => panic!("unexpected error: {error}"),
    }

    cleanup_dir(&root);
}
