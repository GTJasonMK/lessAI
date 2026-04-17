use chrono::Utc;

use crate::{
    models::{SegmentationPreset, RewriteUnitStatus, DocumentSession, EditorSlotEdit, RunningState},
    rewrite_unit::SlotUpdate,
    test_support::{editable_slot, locked_slot, rewrite_unit},
};

use super::{build_plain_text_editor_writeback, build_slot_editor_writeback, EditorWritebackPayload};

fn sample_docx_session() -> DocumentSession {
    let now = Utc::now();
    DocumentSession {
        id: "session-1".to_string(),
        title: "示例".to_string(),
        document_path: "/tmp/example.docx".to_string(),
        source_text: "前文[公式]后文".to_string(),
        source_snapshot: None,
        normalized_text: "前文[公式]后文".to_string(),
        write_back_supported: true,
        write_back_block_reason: None,
        plain_text_editor_safe: true,
        plain_text_editor_block_reason: None,
        segmentation_preset: Some(SegmentationPreset::Paragraph),
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
            RewriteUnitStatus::Idle,
        )],
        suggestions: Vec::new(),
        next_suggestion_sequence: 1,
        status: RunningState::Idle,
        created_at: now,
        updated_at: now,
    }
}

fn sample_text_session() -> DocumentSession {
    let now = Utc::now();
    DocumentSession {
        id: "session-text".to_string(),
        title: "示例".to_string(),
        document_path: "/tmp/example.txt".to_string(),
        source_text: "原文\r\n下一行\r\n".to_string(),
        source_snapshot: None,
        normalized_text: "原文\r\n下一行\r\n".to_string(),
        write_back_supported: true,
        write_back_block_reason: None,
        plain_text_editor_safe: true,
        plain_text_editor_block_reason: None,
        segmentation_preset: Some(SegmentationPreset::Paragraph),
        rewrite_headings: Some(false),
        writeback_slots: vec![editable_slot("slot-0", 0, "原文\r\n下一行\r\n")],
        rewrite_units: vec![rewrite_unit(
            "unit-0",
            0,
            &["slot-0"],
            "原文\r\n下一行\r\n",
            RewriteUnitStatus::Idle,
        )],
        suggestions: Vec::new(),
        next_suggestion_sequence: 1,
        status: RunningState::Idle,
        created_at: now,
        updated_at: now,
    }
}

#[test]
fn build_slot_editor_writeback_returns_updated_slots_for_docx() {
    let session = sample_docx_session();
    let edits = vec![
        EditorSlotEdit {
            slot_id: "slot-0".to_string(),
            text: "新前文".to_string(),
        },
        EditorSlotEdit {
            slot_id: "slot-2".to_string(),
            text: "新后文".to_string(),
        },
    ];

    let payload = build_slot_editor_writeback(&session, &edits).expect("slot writeback");

    match payload {
        EditorWritebackPayload::Slots(slots) => {
            let updates = vec![
                SlotUpdate::new("slot-0", "新前文"),
                SlotUpdate::new("slot-2", "新后文"),
            ];
            let merged = crate::rewrite_unit::apply_slot_updates(&session.writeback_slots, &updates)
                .expect("expected updates to be applicable");
            assert_eq!(slots, merged);
            assert_eq!(slots[0].text, "新前文");
            assert_eq!(slots[1].text, "[公式]");
            assert_eq!(slots[2].text, "新后文");
        }
        EditorWritebackPayload::Text(_) => panic!("docx slot editor should return slots"),
    }
}

#[test]
fn build_slot_editor_writeback_rejects_missing_editable_slot() {
    let session = sample_docx_session();
    let edits = vec![EditorSlotEdit {
        slot_id: "slot-0".to_string(),
        text: "只改一半".to_string(),
    }];

    let error = build_slot_editor_writeback(&session, &edits)
        .expect_err("missing editable slot should fail");

    assert!(error.contains("数量"));
}

#[test]
fn build_slot_editor_writeback_rejects_locked_slot_edit() {
    let session = sample_docx_session();
    let edits = vec![
        EditorSlotEdit {
            slot_id: "slot-0".to_string(),
            text: "新前文".to_string(),
        },
        EditorSlotEdit {
            slot_id: "slot-1".to_string(),
            text: "改公式".to_string(),
        },
    ];

    let error = build_slot_editor_writeback(&session, &edits)
        .expect_err("locked slot edit should fail");

    assert!(error.contains("不可编辑") || error.contains("不存在"));
}

#[test]
fn build_slot_editor_writeback_rejects_non_docx_session() {
    let session = sample_text_session();
    let edits = vec![EditorSlotEdit {
        slot_id: "slot-0".to_string(),
        text: "改写".to_string(),
    }];

    let error = build_slot_editor_writeback(&session, &edits)
        .expect_err("non-docx slot editing should fail");

    assert_eq!(error, "当前仅 docx 支持按槽位编辑写回。");
}

#[test]
fn build_plain_text_editor_writeback_normalizes_line_endings() {
    let session = sample_text_session();

    let payload = build_plain_text_editor_writeback(&session, "新文\n下一行  \n")
        .expect("plain-text writeback should normalize");

    match payload {
        EditorWritebackPayload::Text(text) => assert_eq!(text, "新文\r\n下一行\r\n"),
        EditorWritebackPayload::Slots(_) => panic!("plain-text editor should return text payload"),
    }
}

#[test]
fn build_plain_text_editor_writeback_rejects_dirty_session() {
    let mut session = sample_text_session();
    session.status = RunningState::Completed;
    session.suggestions.push(crate::test_support::rewrite_suggestion(
        "suggestion-1",
        1,
        "unit-0",
        "原文\r\n下一行\r\n",
        "改写后",
        crate::models::SuggestionDecision::Proposed,
        vec![SlotUpdate::new("slot-0", "改写后")],
    ));

    let error = build_plain_text_editor_writeback(&session, "新文")
        .expect_err("dirty editor session should fail");

    assert!(error.contains("覆写并清理记录") || error.contains("重置记录"));
}
