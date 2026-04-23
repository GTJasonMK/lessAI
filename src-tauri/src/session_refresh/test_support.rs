use chrono::Utc;

use crate::{
    documents::LoadedDocumentSource,
    models::{
        DiffResult, DocumentSession, DocumentSnapshot, RewriteUnitStatus, RunningState,
        SegmentationPreset,
        SuggestionDecision,
    },
    rewrite_unit::{RewriteSuggestion, RewriteUnit, SlotUpdate, WritebackSlot},
    session_capability_models::{CapabilityGate, DocumentSessionCapabilities},
};

pub(super) fn sample_session() -> DocumentSession {
    let now = Utc::now();
    let mut session = DocumentSession {
        id: "session-1".to_string(),
        title: "示例".to_string(),
        document_path: "/tmp/example.docx".to_string(),
        source_text: "前文E=mc^2后文".to_string(),
        source_snapshot: None,
        template_kind: None,
        template_signature: None,
        slot_structure_signature: None,
        template_snapshot: None,
        normalized_text: "前文E=mc^2后文".to_string(),
        capabilities: DocumentSessionCapabilities {
            source_writeback: CapabilityGate::allowed(),
            editor_writeback: CapabilityGate::blocked(
                "当前文档包含行内锁定内容（如公式、分页符或占位符），暂不支持在纯文本编辑器中直接写回。",
            ),
            ..Default::default()
        },
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
    };
    crate::documents::hydrate_session_capabilities(&mut session);
    session
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
        diff: DiffResult::default(),
        decision: SuggestionDecision::Applied,
        slot_updates: vec![
            SlotUpdate::new("slot-0", "改写后"),
            SlotUpdate::new("slot-2", "正文"),
        ],
        created_at: now,
        updated_at: now,
    });
    session.status = RunningState::Completed;
    crate::documents::hydrate_session_capabilities(&mut session);
    session
}

pub(super) fn loaded_docx() -> LoadedDocumentSource {
    LoadedDocumentSource {
        source_text: "前文E=mc^2后文".to_string(),
        template_kind: None,
        template_signature: None,
        slot_structure_signature: None,
        template_snapshot: None,
        writeback_slots: vec![
            WritebackSlot::editable("slot-0", 0, "前文"),
            WritebackSlot::locked("slot-1", 1, "E=mc^2"),
            WritebackSlot::editable("slot-2", 2, "后文"),
        ],
        capability_policy: crate::documents::DocumentCapabilityPolicy::new(
            crate::documents::capability_gate(true, None),
            crate::documents::capability_gate(true, None),
        ),
    }
}
