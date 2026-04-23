use chrono::Utc;

use crate::{
    models::{
        DocumentSession, EditorSlotEdit, RewriteUnitStatus, RunningState, SegmentationPreset,
    },
    rewrite_unit::SlotUpdate,
    test_support::{editable_slot, locked_slot, rewrite_unit},
};

use super::{
    build_full_text_editor_writeback, build_slot_editor_writeback, EditorWritebackPayload,
};

fn sample_docx_session() -> DocumentSession {
    let now = Utc::now();
    let mut session = DocumentSession {
        id: "session-1".to_string(),
        title: "示例".to_string(),
        document_path: "/tmp/example.docx".to_string(),
        source_text: "前文[公式]后文".to_string(),
        source_snapshot: None,
        template_kind: None,
        template_signature: None,
        slot_structure_signature: None,
        template_snapshot: None,
        normalized_text: "前文[公式]后文".to_string(),
        capabilities: crate::session_capability_models::DocumentSessionCapabilities {
            source_writeback: crate::session_capability_models::CapabilityGate::allowed(),
            editor_writeback: crate::session_capability_models::CapabilityGate::allowed(),
            ..Default::default()
        },
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
    };
    crate::documents::hydrate_session_capabilities(&mut session);
    session
}

fn sample_text_session() -> DocumentSession {
    let now = Utc::now();
    let mut session = DocumentSession {
        id: "session-text".to_string(),
        title: "示例".to_string(),
        document_path: "/tmp/example.txt".to_string(),
        source_text: "原文\r\n下一行\r\n".to_string(),
        source_snapshot: None,
        template_kind: None,
        template_signature: None,
        slot_structure_signature: None,
        template_snapshot: None,
        normalized_text: "原文\r\n下一行\r\n".to_string(),
        capabilities: crate::session_capability_models::DocumentSessionCapabilities {
            source_writeback: crate::session_capability_models::CapabilityGate::allowed(),
            editor_writeback: crate::session_capability_models::CapabilityGate::allowed(),
            ..Default::default()
        },
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
    };
    crate::documents::hydrate_session_capabilities(&mut session);
    session
}

fn sample_markdown_session() -> DocumentSession {
    let now = Utc::now();
    let mut session = DocumentSession {
        id: "session-markdown".to_string(),
        title: "示例".to_string(),
        document_path: "/tmp/example.md".to_string(),
        source_text: "# 标题\n正文里的 `code`。\n".to_string(),
        source_snapshot: None,
        template_kind: Some("markdown".to_string()),
        template_signature: Some("template-markdown".to_string()),
        slot_structure_signature: Some("slot-structure-markdown".to_string()),
        template_snapshot: None,
        normalized_text: "# 标题\n正文里的 `code`。\n".to_string(),
        capabilities: crate::session_capability_models::DocumentSessionCapabilities {
            source_writeback: crate::session_capability_models::CapabilityGate::allowed(),
            editor_writeback: crate::session_capability_models::CapabilityGate::allowed(),
            ..Default::default()
        },
        segmentation_preset: Some(SegmentationPreset::Paragraph),
        rewrite_headings: Some(false),
        writeback_slots: vec![
            locked_slot("slot-0", 0, "# "),
            editable_slot("slot-1", 1, "标题"),
            editable_slot("slot-2", 2, "正文里的 "),
            locked_slot("slot-3", 3, "`code`"),
            editable_slot("slot-4", 4, "。"),
        ],
        rewrite_units: vec![
            rewrite_unit("unit-0", 0, &["slot-0", "slot-1"], "# 标题", RewriteUnitStatus::Idle),
            rewrite_unit(
                "unit-1",
                1,
                &["slot-2", "slot-3", "slot-4"],
                "正文里的 `code`。",
                RewriteUnitStatus::Idle,
            ),
        ],
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
            let merged =
                crate::rewrite_unit::apply_slot_updates(&session.writeback_slots, &updates)
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

    let error =
        build_slot_editor_writeback(&session, &edits).expect_err("locked slot edit should fail");

    assert!(error.contains("不可编辑") || error.contains("不存在"));
}

#[test]
fn build_slot_editor_writeback_rejects_non_slot_based_session() {
    let session = sample_text_session();
    let edits = vec![EditorSlotEdit {
        slot_id: "slot-0".to_string(),
        text: "改写".to_string(),
    }];

    let error = build_slot_editor_writeback(&session, &edits)
        .expect_err("non-slot-based session should fail");

    assert_eq!(error, "当前仅槽位编辑文档支持按槽位写回。");
}

#[test]
fn build_slot_editor_writeback_accepts_markdown_session() {
    let session = sample_markdown_session();
    let edits = vec![
        EditorSlotEdit {
            slot_id: "slot-1".to_string(),
            text: "新标题".to_string(),
        },
        EditorSlotEdit {
            slot_id: "slot-2".to_string(),
            text: "新的正文 ".to_string(),
        },
        EditorSlotEdit {
            slot_id: "slot-4".to_string(),
            text: "！".to_string(),
        },
    ];

    let payload = build_slot_editor_writeback(&session, &edits).expect("markdown slot writeback");

    match payload {
        EditorWritebackPayload::Slots(slots) => {
            assert_eq!(slots[0].text, "# ");
            assert_eq!(slots[1].text, "新标题");
            assert_eq!(slots[2].text, "新的正文 ");
            assert_eq!(slots[3].text, "`code`");
            assert_eq!(slots[4].text, "！");
        }
        EditorWritebackPayload::Text(_) => panic!("markdown slot editor should return slots"),
    }
}

#[test]
fn build_full_text_editor_writeback_rejects_slot_based_markdown_session() {
    let session = sample_markdown_session();

    let error = build_full_text_editor_writeback(&session, "# 新标题\n新正文")
        .expect_err("slot-based markdown session should reject full-text writeback");

    assert_eq!(error, "结构化编辑模式必须按槽位保存，不能再走整篇纯文本写回。");
}

#[test]
fn build_full_text_editor_writeback_normalizes_line_endings() {
    let session = sample_text_session();

    let payload = build_full_text_editor_writeback(&session, "新文\n下一行  \n")
        .expect("plain-text writeback should normalize");

    match payload {
        EditorWritebackPayload::Text(text) => assert_eq!(text, "新文\r\n下一行\r\n"),
        EditorWritebackPayload::Slots(_) => panic!("plain-text editor should return text payload"),
    }
}

#[test]
fn build_full_text_editor_writeback_rejects_dirty_session() {
    let mut session = sample_text_session();
    session.status = RunningState::Completed;
    session
        .suggestions
        .push(crate::test_support::rewrite_suggestion(
            "suggestion-1",
            1,
            "unit-0",
            "原文\r\n下一行\r\n",
            "改写后",
            crate::models::SuggestionDecision::Proposed,
            vec![SlotUpdate::new("slot-0", "改写后")],
        ));
    crate::documents::hydrate_session_capabilities(&mut session);

    let error = build_full_text_editor_writeback(&session, "新文")
        .expect_err("dirty editor session should fail");

    assert!(error.contains("覆写并清理记录") || error.contains("重置记录"));
}
