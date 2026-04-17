use chrono::Utc;

use crate::{
    documents::LoadedDocumentSource,
    models::{
        SegmentationPreset, RewriteUnitStatus, DocumentSession, DocumentSnapshot, RunningState,
        SuggestionDecision,
    },
    rewrite_unit::{RewriteSuggestion, RewriteUnit, SlotUpdate, WritebackSlot},
};

pub(super) fn sample_session() -> DocumentSession {
    let now = Utc::now();
    DocumentSession {
        id: "session-1".to_string(),
        title: "示例".to_string(),
        document_path: "/tmp/example.docx".to_string(),
        source_text: "前文E=mc^2后文".to_string(),
        source_snapshot: None,
        normalized_text: "前文E=mc^2后文".to_string(),
        write_back_supported: true,
        write_back_block_reason: None,
        plain_text_editor_safe: false,
        plain_text_editor_block_reason: Some(
            "当前文档包含行内锁定内容（如公式、分页符或占位符），暂不支持在纯文本编辑器中直接写回。"
                .to_string(),
        ),
        segmentation_preset: Some(SegmentationPreset::Paragraph),
        rewrite_headings: Some(false),
        writeback_slots: vec![
            WritebackSlot::editable("slot-0", 0, "前文"),
            WritebackSlot::locked("slot-1", 1, "E=mc^2"),
            WritebackSlot::editable("slot-2", 2, "后文"),
        ],
        rewrite_units: vec![RewriteUnit {
            id: "unit-0".to_string(),
            order: 0,
            slot_ids: vec![
                "slot-0".to_string(),
                "slot-1".to_string(),
                "slot-2".to_string(),
            ],
            display_text: "前文E=mc^2后文".to_string(),
            segmentation_preset: SegmentationPreset::Paragraph,
            status: RewriteUnitStatus::Idle,
            error_message: None,
        }],
        suggestions: Vec::new(),
        next_suggestion_sequence: 1,
        status: RunningState::Idle,
        created_at: now,
        updated_at: now,
    }
}

pub(super) fn dirty_session_with_applied_suggestion() -> DocumentSession {
    let mut session = sample_session();
    let now = Utc::now();
    session.source_snapshot = Some(DocumentSnapshot {
        sha256: "old".to_string(),
    });
    session.suggestions.push(RewriteSuggestion {
        id: "suggestion-1".to_string(),
        sequence: 1,
        rewrite_unit_id: "unit-0".to_string(),
        before_text: "前文E=mc^2后文".to_string(),
        after_text: "改写后正文".to_string(),
        diff_spans: Vec::new(),
        decision: SuggestionDecision::Applied,
        slot_updates: vec![
            SlotUpdate::new("slot-0", "改写后"),
            SlotUpdate::new("slot-2", "正文"),
        ],
        created_at: now,
        updated_at: now,
    });
    session.status = RunningState::Completed;
    session
}

pub(super) fn loaded_docx() -> LoadedDocumentSource {
    LoadedDocumentSource {
        source_text: "前文E=mc^2后文".to_string(),
        writeback_slots: vec![
            WritebackSlot::editable("slot-0", 0, "前文"),
            WritebackSlot::locked("slot-1", 1, "E=mc^2"),
            WritebackSlot::editable("slot-2", 2, "后文"),
        ],
        write_back_supported: true,
        write_back_block_reason: None,
        plain_text_editor_safe: true,
        plain_text_editor_block_reason: None,
    }
}
