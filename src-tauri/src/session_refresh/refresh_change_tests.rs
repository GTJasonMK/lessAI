use std::path::Path;

use super::refresh_session_from_loaded;
use crate::{
    documents::LoadedDocumentSource,
    models::{DocumentSnapshot, RunningState, SegmentationPreset, SuggestionDecision},
    rewrite_unit::SlotUpdate,
    session_refresh::test_support::{
        dirty_session_with_applied_suggestion, loaded_docx, sample_session,
    },
    test_support::{editable_slot, locked_slot, rewrite_suggestion},
};

#[test]
fn rebuilds_clean_session_when_snapshot_changes_even_if_text_is_same() {
    let mut existing = sample_session();
    existing.source_snapshot = Some(DocumentSnapshot {
        sha256: "old".to_string(),
    });

    let refreshed = refresh_session_from_loaded(
        &existing,
        Path::new("/tmp/example.docx"),
        loaded_docx(),
        SegmentationPreset::Paragraph,
        false,
        Some(DocumentSnapshot {
            sha256: "new".to_string(),
        }),
    );

    assert!(refreshed.changed);
    assert_eq!(
        refreshed
            .session
            .source_snapshot
            .as_ref()
            .map(|item| item.sha256.as_str()),
        Some("new")
    );
    assert_eq!(refreshed.session.writeback_slots.len(), 3);
    assert_eq!(refreshed.session.rewrite_units.len(), 1);
    assert!(refreshed.session.capabilities.source_writeback.allowed);
    assert_eq!(refreshed.session.capabilities.source_writeback.block_reason, None);
}

#[test]
fn blocks_dirty_session_when_snapshot_changes_even_if_text_is_same() {
    let existing = dirty_session_with_applied_suggestion();

    let refreshed = refresh_session_from_loaded(
        &existing,
        Path::new("/tmp/example.docx"),
        loaded_docx(),
        SegmentationPreset::Paragraph,
        false,
        Some(DocumentSnapshot {
            sha256: "new".to_string(),
        }),
    );

    assert!(refreshed.changed);
    assert_eq!(refreshed.session.suggestions.len(), 1);
    assert!(!refreshed.session.capabilities.source_writeback.allowed);
    assert!(!refreshed.session.capabilities.editor_writeback.allowed);
    assert!(refreshed
        .session
        .capabilities
        .source_writeback
        .block_reason
        .as_deref()
        .is_some_and(|reason| reason.contains("外部发生变化")));
    assert_eq!(
        refreshed
            .session
            .source_snapshot
            .as_ref()
            .map(|item| item.sha256.as_str()),
        Some("old")
    );
}

#[test]
fn rebuilds_snapshotless_clean_session_when_source_changes() {
    let existing = sample_session();
    let loaded = LoadedDocumentSource {
        source_text: "新前文E=mc^2新后文".to_string(),
        template_kind: None,
        template_signature: None,
        slot_structure_signature: None,
        template_snapshot: None,
        writeback_slots: vec![
            editable_slot("slot-0", 0, "新前文"),
            locked_slot("slot-1", 1, "E=mc^2"),
            editable_slot("slot-2", 2, "新后文"),
        ],
        capability_policy: crate::documents::DocumentCapabilityPolicy::new(
            crate::documents::capability_gate(true, None),
            crate::documents::capability_gate(true, None),
        ),
    };

    let refreshed = refresh_session_from_loaded(
        &existing,
        Path::new("/tmp/example.docx"),
        loaded,
        SegmentationPreset::Paragraph,
        false,
        Some(DocumentSnapshot {
            sha256: "new".to_string(),
        }),
    );

    assert!(refreshed.changed);
    assert_eq!(refreshed.session.source_text, "新前文E=mc^2新后文");
    assert_eq!(
        refreshed
            .session
            .source_snapshot
            .as_ref()
            .map(|item| item.sha256.as_str()),
        Some("new")
    );
    assert!(refreshed.session.suggestions.is_empty());
}

#[test]
fn blocks_snapshotless_dirty_session_when_source_changes() {
    let mut existing = sample_session();
    existing.suggestions.push(rewrite_suggestion(
        "suggestion-1",
        1,
        "unit-0",
        &existing.source_text,
        "改写后正文",
        SuggestionDecision::Applied,
        vec![
            SlotUpdate::new("slot-0", "改写后"),
            SlotUpdate::new("slot-2", "正文"),
        ],
    ));
    existing.status = RunningState::Completed;
    crate::documents::hydrate_session_capabilities(&mut existing);

    let loaded = LoadedDocumentSource {
        source_text: "新前文E=mc^2新后文".to_string(),
        template_kind: None,
        template_signature: None,
        slot_structure_signature: None,
        template_snapshot: None,
        writeback_slots: vec![editable_slot("slot-0", 0, "新前文E=mc^2新后文")],
        capability_policy: crate::documents::DocumentCapabilityPolicy::new(
            crate::documents::capability_gate(true, None),
            crate::documents::capability_gate(true, None),
        ),
    };

    let refreshed = refresh_session_from_loaded(
        &existing,
        Path::new("/tmp/example.docx"),
        loaded,
        SegmentationPreset::Paragraph,
        false,
        Some(DocumentSnapshot {
            sha256: "new".to_string(),
        }),
    );

    assert!(refreshed.changed);
    assert_eq!(refreshed.session.suggestions.len(), 1);
    assert!(!refreshed.session.capabilities.source_writeback.allowed);
    assert!(!refreshed.session.capabilities.editor_writeback.allowed);
    assert_eq!(refreshed.session.source_snapshot, None);
}

#[test]
fn blocks_snapshotless_dirty_session_even_when_source_text_is_unchanged() {
    let mut existing = sample_session();
    existing.suggestions.push(rewrite_suggestion(
        "suggestion-1",
        1,
        "unit-0",
        &existing.source_text,
        "改写后正文",
        SuggestionDecision::Applied,
        vec![
            SlotUpdate::new("slot-0", "改写后"),
            SlotUpdate::new("slot-2", "正文"),
        ],
    ));
    existing.status = RunningState::Completed;
    crate::documents::hydrate_session_capabilities(&mut existing);

    let refreshed = refresh_session_from_loaded(
        &existing,
        Path::new("/tmp/example.docx"),
        loaded_docx(),
        SegmentationPreset::Paragraph,
        false,
        Some(DocumentSnapshot {
            sha256: "new".to_string(),
        }),
    );

    assert!(refreshed.changed);
    assert_eq!(refreshed.session.suggestions.len(), 1);
    assert!(!refreshed.session.capabilities.source_writeback.allowed);
    assert!(!refreshed.session.capabilities.editor_writeback.allowed);
    assert_eq!(refreshed.session.source_snapshot, None);
}
