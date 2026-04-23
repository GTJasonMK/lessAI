use chrono::Utc;

use crate::{
    adapters::docx::DocxAdapter,
    document_snapshot::capture_document_snapshot,
    models::{DocumentSession, RunningState, SegmentationPreset, SuggestionDecision},
    rewrite_unit::{build_rewrite_units, SlotUpdate},
    test_support::{build_minimal_docx, cleanup_dir, rewrite_suggestion, write_temp_file},
};

fn session_from_docx(path: &std::path::Path, bytes: &[u8]) -> DocumentSession {
    let now = Utc::now();
    let model = DocxAdapter::extract_writeback_model(bytes, false).expect("extract model");
    let source_text = model.source_text.clone();

    let mut session = DocumentSession {
        id: "fixture-session".to_string(),
        title: "示例".to_string(),
        document_path: path.to_string_lossy().to_string(),
        source_text: source_text.clone(),
        source_snapshot: Some(capture_document_snapshot(path).expect("capture snapshot")),
        template_kind: None,
        template_signature: Some(model.template_signature),
        slot_structure_signature: Some(model.slot_structure_signature),
        template_snapshot: None,
        normalized_text: source_text,
        capabilities: crate::session_capability_models::DocumentSessionCapabilities {
            source_writeback: crate::session_capability_models::CapabilityGate::allowed(),
            editor_writeback: crate::session_capability_models::CapabilityGate::blocked(
                "docx 仅支持槽位编辑",
            ),
            ..Default::default()
        },
        segmentation_preset: Some(SegmentationPreset::Paragraph),
        rewrite_headings: Some(false),
        rewrite_units: build_rewrite_units(&model.writeback_slots, SegmentationPreset::Paragraph),
        writeback_slots: model.writeback_slots,
        suggestions: Vec::new(),
        next_suggestion_sequence: 1,
        status: RunningState::Idle,
        created_at: now,
        updated_at: now,
    };
    crate::documents::hydrate_session_capabilities(&mut session);
    session
}

#[test]
fn execute_session_writeback_validates_docx_with_adjacent_styles() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:rPr><w:b/></w:rPr><w:t>前文</w:t></w:r>
      <w:r><w:rPr><w:u w:val="single"/></w:rPr><w:t>后文</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let (root, target) = write_temp_file("fixture-adjacent-style", "docx", &bytes);
    let mut session = session_from_docx(&target, &bytes);
    session.suggestions.push(rewrite_suggestion(
        "suggestion-1",
        1,
        "unit-0",
        "前文后文",
        "新前文新后文",
        SuggestionDecision::Applied,
        vec![
            SlotUpdate::new("docx:p0:r0", "新前文"),
            SlotUpdate::new("docx:p0:r1", "新后文"),
        ],
    ));

    super::execute_session_writeback(&session, crate::documents::WritebackMode::Validate)
        .expect("adjacent styled slots should validate");

    cleanup_dir(&root);
}

#[test]
fn validate_candidate_batch_writeback_accepts_updates_scoped_to_one_unit() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:t>前文</w:t></w:r>
      <w:r><w:t>[图表]</w:t></w:r>
      <w:r><w:t>后文</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let (root, target) = write_temp_file("fixture-candidate-scope", "docx", &bytes);
    let session = session_from_docx(&target, &bytes);
    let unit = session
        .rewrite_units
        .iter()
        .find(|item| item.id == "unit-0")
        .expect("unit-0");
    let editable_ids = unit
        .slot_ids
        .iter()
        .filter(|slot_id| {
            session
                .writeback_slots
                .iter()
                .find(|slot| &slot.id == *slot_id)
                .is_some_and(|slot| slot.editable)
        })
        .cloned()
        .collect::<Vec<_>>();

    super::validate_candidate_batch_writeback(
        &session,
        &[crate::rewrite_unit::RewriteUnitResponse {
            rewrite_unit_id: "unit-0".to_string(),
            updates: if editable_ids.len() == 1 {
                vec![SlotUpdate::new(&editable_ids[0], "新前文[图表]新后文")]
            } else {
                vec![
                    SlotUpdate::new(&editable_ids[0], "新前文"),
                    SlotUpdate::new(&editable_ids[1], "新后文"),
                ]
            },
        }],
    )
    .expect("slot-scoped candidate should validate");

    cleanup_dir(&root);
}

#[test]
fn validate_candidate_batch_writeback_rejects_conflicting_slot_updates_across_units() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>第一段</w:t></w:r></w:p>
    <w:p><w:r><w:t>第二段</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let (root, target) = write_temp_file("fixture-conflicting-batch-slot", "docx", &bytes);
    let session = session_from_docx(&target, &bytes);

    let error = super::validate_candidate_batch_writeback(
        &session,
        &[
            crate::rewrite_unit::RewriteUnitResponse {
                rewrite_unit_id: "unit-0".to_string(),
                updates: vec![SlotUpdate::new("docx:p0:r0", "改写第一段")],
            },
            crate::rewrite_unit::RewriteUnitResponse {
                rewrite_unit_id: "unit-1".to_string(),
                updates: vec![SlotUpdate::new("docx:p0:r0", "冲突改写")],
            },
        ],
    )
    .expect_err("conflicting slot updates across units should fail");

    assert!(error.contains("slot"));
    cleanup_dir(&root);
}
