use std::path::Path;

use super::refresh_session_from_loaded;
use crate::{
    documents::LoadedDocumentSource,
    models::{SegmentationPreset, RewriteUnitStatus, DocumentSnapshot},
    rewrite_unit::RewriteUnit,
    session_refresh::test_support::{
        dirty_session_with_applied_suggestion, loaded_docx, sample_session,
    },
    test_support::editable_slot,
};

#[test]
fn refreshes_stale_plain_text_editor_capability() {
    let existing = sample_session();

    let refreshed = refresh_session_from_loaded(
        &existing,
        Path::new("/tmp/example.docx"),
        loaded_docx(),
        SegmentationPreset::Paragraph,
        false,
        Some(DocumentSnapshot {
            sha256: "abc".to_string(),
        }),
    );

    assert!(refreshed.changed);
    assert!(refreshed.session.plain_text_editor_safe);
    assert_eq!(refreshed.session.plain_text_editor_block_reason, None);
    assert_eq!(
        refreshed
            .session
            .source_snapshot
            .as_ref()
            .map(|item| item.sha256.as_str()),
        Some("abc")
    );
}

#[test]
fn rebuilds_clean_session_when_segmentation_preset_metadata_is_missing() {
    let now = chrono::Utc::now();
    let existing = crate::models::DocumentSession {
        id: "session-2".to_string(),
        title: "示例".to_string(),
        document_path: "/tmp/example.docx".to_string(),
        source_text: "第一句。第二句。".to_string(),
        source_snapshot: None,
        normalized_text: "第一句。第二句。".to_string(),
        write_back_supported: true,
        write_back_block_reason: None,
        plain_text_editor_safe: true,
        plain_text_editor_block_reason: None,
        segmentation_preset: None,
        rewrite_headings: None,
        writeback_slots: Vec::new(),
        rewrite_units: Vec::new(),
        suggestions: Vec::new(),
        next_suggestion_sequence: 1,
        status: crate::models::RunningState::Idle,
        created_at: now,
        updated_at: now,
    };
    let loaded = LoadedDocumentSource {
        source_text: "第一句。第二句。".to_string(),
        writeback_slots: vec![editable_slot("slot-0", 0, "第一句。第二句。")],
        write_back_supported: true,
        write_back_block_reason: None,
        plain_text_editor_safe: true,
        plain_text_editor_block_reason: None,
    };

    let refreshed = refresh_session_from_loaded(
        &existing,
        Path::new("/tmp/example.docx"),
        loaded,
        SegmentationPreset::Paragraph,
        false,
        None,
    );

    assert!(refreshed.changed);
    assert_eq!(refreshed.session.segmentation_preset, Some(SegmentationPreset::Paragraph));
    assert_eq!(refreshed.session.rewrite_headings, Some(false));
    assert_eq!(refreshed.session.rewrite_units.len(), 1);
    assert_eq!(refreshed.session.rewrite_units[0].display_text, "第一句。第二句。");
}

#[test]
fn rebuilds_clean_docx_session_when_chunk_structure_is_stale() {
    let mut existing = sample_session();
    existing.source_snapshot = Some(DocumentSnapshot {
        sha256: "same".to_string(),
    });

    let refreshed = refresh_session_from_loaded(
        &existing,
        Path::new("/tmp/example.docx"),
        loaded_docx(),
        SegmentationPreset::Paragraph,
        false,
        Some(DocumentSnapshot {
            sha256: "same".to_string(),
        }),
    );

    assert!(refreshed.changed);
    assert_eq!(refreshed.session.writeback_slots.len(), 3);
    assert_eq!(refreshed.session.writeback_slots[0].text, "前文");
    assert!(refreshed.session.writeback_slots[0].editable);
    assert_eq!(refreshed.session.writeback_slots[1].text, "E=mc^2");
    assert!(!refreshed.session.writeback_slots[1].editable);
    assert_eq!(refreshed.session.writeback_slots[2].text, "后文");
    assert!(refreshed.session.writeback_slots[2].editable);
}

#[test]
fn rebuilds_clean_session_when_rewrite_units_change() {
    let now = chrono::Utc::now();
    let existing = crate::models::DocumentSession {
        id: "session-3".to_string(),
        title: "示例".to_string(),
        document_path: "/tmp/example.docx".to_string(),
        source_text: "第一句。第二句。".to_string(),
        source_snapshot: None,
        normalized_text: "第一句。第二句。".to_string(),
        write_back_supported: true,
        write_back_block_reason: None,
        plain_text_editor_safe: true,
        plain_text_editor_block_reason: None,
        segmentation_preset: Some(SegmentationPreset::Paragraph),
        rewrite_headings: Some(false),
        writeback_slots: vec![editable_slot("slot-0", 0, "第一句。第二句。")],
        rewrite_units: vec![
            RewriteUnit {
                id: "unit-0".to_string(),
                order: 0,
                slot_ids: vec!["slot-0".to_string()],
                display_text: "第一句。".to_string(),
                segmentation_preset: SegmentationPreset::Paragraph,
                status: RewriteUnitStatus::Idle,
                error_message: None,
            },
            RewriteUnit {
                id: "unit-1".to_string(),
                order: 1,
                slot_ids: vec!["slot-0".to_string()],
                display_text: "第二句。".to_string(),
                segmentation_preset: SegmentationPreset::Paragraph,
                status: RewriteUnitStatus::Idle,
                error_message: None,
            },
        ],
        suggestions: Vec::new(),
        next_suggestion_sequence: 1,
        status: crate::models::RunningState::Idle,
        created_at: now,
        updated_at: now,
    };
    let loaded = LoadedDocumentSource {
        source_text: "第一句。第二句。".to_string(),
        writeback_slots: vec![editable_slot("slot-0", 0, "第一句。第二句。")],
        write_back_supported: true,
        write_back_block_reason: None,
        plain_text_editor_safe: true,
        plain_text_editor_block_reason: None,
    };

    let refreshed = refresh_session_from_loaded(
        &existing,
        Path::new("/tmp/example.docx"),
        loaded,
        SegmentationPreset::Paragraph,
        false,
        None,
    );

    assert!(refreshed.changed);
    assert_eq!(refreshed.session.rewrite_units.len(), 1);
    assert_eq!(
        refreshed.session.rewrite_units[0].slot_ids,
        vec!["slot-0".to_string()]
    );
    assert_eq!(refreshed.session.rewrite_units[0].display_text, "第一句。第二句。");
}

#[test]
fn blocks_dirty_docx_session_when_chunk_structure_is_stale() {
    let mut existing = dirty_session_with_applied_suggestion();
    existing.source_snapshot = Some(DocumentSnapshot {
        sha256: "same".to_string(),
    });
    existing.writeback_slots = vec![editable_slot("slot-0", 0, "前文E=mc^2后文")];
    existing.rewrite_units = vec![RewriteUnit {
        id: "unit-0".to_string(),
        order: 0,
        slot_ids: vec!["slot-0".to_string()],
        display_text: "前文E=mc^2后文".to_string(),
        segmentation_preset: SegmentationPreset::Paragraph,
        status: RewriteUnitStatus::Done,
        error_message: None,
    }];

    let refreshed = refresh_session_from_loaded(
        &existing,
        Path::new("/tmp/example.docx"),
        loaded_docx(),
        SegmentationPreset::Paragraph,
        false,
        Some(DocumentSnapshot {
            sha256: "same".to_string(),
        }),
    );

    assert!(refreshed.changed);
    assert_eq!(refreshed.session.suggestions.len(), 1);
    assert!(!refreshed.session.write_back_supported);
    assert!(!refreshed.session.plain_text_editor_safe);
    assert!(refreshed
        .session
        .write_back_block_reason
        .as_deref()
        .is_some_and(|reason| reason.contains("分块结构")));
}
