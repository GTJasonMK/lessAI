use std::path::Path;

use super::refresh_session_from_loaded;
use crate::{
    adapters::TextRegion,
    documents::writeback_slots_from_regions,
    documents::LoadedDocumentSource,
    models::{DocumentSnapshot, RewriteUnitStatus, RunningState, SegmentationPreset},
    rewrite_unit::RewriteUnit,
    session_refresh::test_support::{
        dirty_session_with_applied_suggestion, loaded_docx, sample_session,
    },
    test_support::editable_slot,
};

#[test]
fn refreshes_stale_editor_writeback_capability() {
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
    assert!(refreshed.session.capabilities.editor_writeback.allowed);
    assert_eq!(
        refreshed.session.capabilities.editor_writeback.block_reason,
        None
    );
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
    let mut existing = crate::models::DocumentSession {
        id: "session-2".to_string(),
        title: "示例".to_string(),
        document_path: "/tmp/example.docx".to_string(),
        source_text: "第一句。第二句。".to_string(),
        source_snapshot: None,
        template_kind: None,
        template_signature: None,
        slot_structure_signature: None,
        template_snapshot: None,
        normalized_text: "第一句。第二句。".to_string(),
        capabilities: crate::session_capability_models::DocumentSessionCapabilities {
            source_writeback: crate::session_capability_models::CapabilityGate::allowed(),
            editor_writeback: crate::session_capability_models::CapabilityGate::allowed(),
            ..Default::default()
        },
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
    crate::documents::hydrate_session_capabilities(&mut existing);
    let loaded = LoadedDocumentSource {
        source_text: "第一句。第二句。".to_string(),
        template_kind: None,
        template_signature: None,
        slot_structure_signature: None,
        template_snapshot: None,
        writeback_slots: vec![editable_slot("slot-0", 0, "第一句。第二句。")],
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
        None,
    );

    assert!(refreshed.changed);
    assert_eq!(
        refreshed.session.segmentation_preset,
        Some(SegmentationPreset::Paragraph)
    );
    assert_eq!(refreshed.session.rewrite_headings, Some(false));
    assert_eq!(refreshed.session.rewrite_units.len(), 1);
    assert_eq!(
        refreshed.session.rewrite_units[0].display_text,
        "第一句。第二句。"
    );
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
    let mut existing = crate::models::DocumentSession {
        id: "session-3".to_string(),
        title: "示例".to_string(),
        document_path: "/tmp/example.docx".to_string(),
        source_text: "第一句。第二句。".to_string(),
        source_snapshot: None,
        template_kind: None,
        template_signature: None,
        slot_structure_signature: None,
        template_snapshot: None,
        normalized_text: "第一句。第二句。".to_string(),
        capabilities: crate::session_capability_models::DocumentSessionCapabilities {
            source_writeback: crate::session_capability_models::CapabilityGate::allowed(),
            editor_writeback: crate::session_capability_models::CapabilityGate::allowed(),
            ..Default::default()
        },
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
    crate::documents::hydrate_session_capabilities(&mut existing);
    let loaded = LoadedDocumentSource {
        source_text: "第一句。第二句。".to_string(),
        template_kind: None,
        template_signature: None,
        slot_structure_signature: None,
        template_snapshot: None,
        writeback_slots: vec![editable_slot("slot-0", 0, "第一句。第二句。")],
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
        None,
    );

    assert!(refreshed.changed);
    assert_eq!(refreshed.session.rewrite_units.len(), 1);
    assert_eq!(
        refreshed.session.rewrite_units[0].slot_ids,
        vec!["slot-0".to_string()]
    );
    assert_eq!(
        refreshed.session.rewrite_units[0].display_text,
        "第一句。第二句。"
    );
}

#[test]
fn rebuilds_clean_text_session_when_segmentation_preset_changes() {
    let now = chrono::Utc::now();
    let text = "第一句。第二句，第三句。";
    let mut existing = crate::models::DocumentSession {
        id: "session-text-1".to_string(),
        title: "示例".to_string(),
        document_path: "/tmp/example.tex".to_string(),
        source_text: text.to_string(),
        source_snapshot: None,
        template_kind: None,
        template_signature: None,
        slot_structure_signature: None,
        template_snapshot: None,
        normalized_text: text.to_string(),
        capabilities: crate::session_capability_models::DocumentSessionCapabilities {
            source_writeback: crate::session_capability_models::CapabilityGate::allowed(),
            editor_writeback: crate::session_capability_models::CapabilityGate::allowed(),
            ..Default::default()
        },
        segmentation_preset: Some(SegmentationPreset::Paragraph),
        rewrite_headings: Some(false),
        writeback_slots: vec![editable_slot("slot-0", 0, text)],
        rewrite_units: vec![RewriteUnit {
            id: "unit-0".to_string(),
            order: 0,
            slot_ids: vec!["slot-0".to_string()],
            display_text: text.to_string(),
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
    crate::documents::hydrate_session_capabilities(&mut existing);
    let loaded = LoadedDocumentSource {
        source_text: text.to_string(),
        template_kind: None,
        template_signature: None,
        slot_structure_signature: None,
        template_snapshot: None,
        writeback_slots: writeback_slots_from_regions(&[TextRegion::editable(text)]),
        capability_policy: crate::documents::DocumentCapabilityPolicy::new(
            crate::documents::capability_gate(true, None),
            crate::documents::capability_gate(true, None),
        ),
    };

    let refreshed = refresh_session_from_loaded(
        &existing,
        Path::new("/tmp/example.tex"),
        loaded,
        SegmentationPreset::Sentence,
        false,
        None,
    );

    assert!(refreshed.changed);
    assert_eq!(
        refreshed.session.segmentation_preset,
        Some(SegmentationPreset::Sentence)
    );
    assert_eq!(refreshed.session.rewrite_units.len(), 2);
    assert_eq!(refreshed.session.rewrite_units[0].display_text, "第一句。");
    assert_eq!(
        refreshed.session.rewrite_units[1].display_text,
        "第二句，第三句。"
    );
}

#[test]
fn rebuilds_clean_session_when_template_signature_changes() {
    let now = chrono::Utc::now();
    let template = crate::textual_template::models::TextTemplate::single_paragraph(
        "plain_text",
        "txt:p0",
        "第一句。",
    );
    let built = crate::textual_template::slots::build_slots(&template);
    let mut existing = crate::models::DocumentSession {
        id: "session-template-1".to_string(),
        title: "示例".to_string(),
        document_path: "/tmp/example.txt".to_string(),
        source_text: "第一句。".to_string(),
        source_snapshot: None,
        template_kind: Some("plain_text".to_string()),
        template_signature: Some("old-template-signature".to_string()),
        slot_structure_signature: Some(built.slot_structure_signature.clone()),
        template_snapshot: Some(template.clone()),
        normalized_text: "第一句。".to_string(),
        capabilities: crate::session_capability_models::DocumentSessionCapabilities {
            source_writeback: crate::session_capability_models::CapabilityGate::allowed(),
            editor_writeback: crate::session_capability_models::CapabilityGate::allowed(),
            ..Default::default()
        },
        segmentation_preset: Some(SegmentationPreset::Paragraph),
        rewrite_headings: Some(false),
        writeback_slots: built.slots.clone(),
        rewrite_units: vec![RewriteUnit {
            id: "unit-0".to_string(),
            order: 0,
            slot_ids: vec!["txt:p0:r0:s0".to_string()],
            display_text: "第一句。".to_string(),
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
    crate::documents::hydrate_session_capabilities(&mut existing);
    let loaded = LoadedDocumentSource {
        source_text: "第一句。".to_string(),
        template_kind: Some("plain_text".to_string()),
        template_signature: Some(template.template_signature.clone()),
        slot_structure_signature: Some(built.slot_structure_signature.clone()),
        template_snapshot: Some(template),
        writeback_slots: built.slots,
        capability_policy: crate::documents::DocumentCapabilityPolicy::new(
            crate::documents::capability_gate(true, None),
            crate::documents::capability_gate(true, None),
        ),
    };

    let refreshed = refresh_session_from_loaded(
        &existing,
        Path::new("/tmp/example.txt"),
        loaded,
        SegmentationPreset::Paragraph,
        false,
        None,
    );

    assert!(refreshed.changed);
    assert_ne!(
        refreshed.session.template_signature.as_deref(),
        Some("old-template-signature")
    );
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
    assert!(!refreshed.session.capabilities.source_writeback.allowed);
    assert!(!refreshed.session.capabilities.editor_writeback.allowed);
    assert!(refreshed
        .session
        .capabilities
        .source_writeback
        .block_reason
        .as_deref()
        .is_some_and(|reason| reason.contains("分块结构")));
}

#[test]
fn keeps_clean_docx_session_when_only_template_kind_none_vs_docx_differs() {
    let mut existing = sample_session();
    existing.template_kind = None;
    existing.template_signature = Some("sig-docx".to_string());
    existing.slot_structure_signature = Some("slot-docx".to_string());
    existing.source_snapshot = Some(DocumentSnapshot {
        sha256: "same".to_string(),
    });
    crate::documents::hydrate_session_capabilities(&mut existing);

    let mut loaded = loaded_docx();
    loaded.template_kind = Some("docx".to_string());
    loaded.template_signature = Some("sig-docx".to_string());
    loaded.slot_structure_signature = Some("slot-docx".to_string());

    let refreshed = refresh_session_from_loaded(
        &existing,
        Path::new("/tmp/example.docx"),
        loaded,
        SegmentationPreset::Paragraph,
        false,
        Some(DocumentSnapshot {
            sha256: "same".to_string(),
        }),
    );

    assert!(refreshed.changed);
    assert_eq!(refreshed.session.template_kind.as_deref(), Some("docx"));
    assert!(refreshed.session.capabilities.source_writeback.allowed);
    assert!(refreshed.session.capabilities.editor_writeback.allowed);
}
