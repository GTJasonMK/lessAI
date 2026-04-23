use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
};

use chrono::Utc;
use uuid::Uuid;
use zip::{write::FileOptions, ZipWriter};

use crate::{
    models::{
        DiffResult, DocumentSession, RewriteUnitStatus, RunningState, SegmentationPreset,
        SuggestionDecision,
    },
    rewrite_unit::{RewriteSuggestion, RewriteUnit, SlotUpdate, WritebackSlot},
    session_capability_models::{CapabilityGate, DocumentSessionCapabilities},
};

pub(crate) fn unique_test_dir(name: &str) -> PathBuf {
    env::temp_dir().join(format!("lessai-{name}-{}", Uuid::new_v4()))
}

pub(crate) fn cleanup_dir(path: &Path) {
    let _ = fs::remove_dir_all(path);
}

pub(crate) fn write_temp_file(name: &str, ext: &str, contents: &[u8]) -> (PathBuf, PathBuf) {
    let root = unique_test_dir(name);
    fs::create_dir_all(&root).expect("create root");
    let target = root.join(format!("sample.{ext}"));
    fs::write(&target, contents).expect("write temp file");
    (root, target)
}

pub(crate) fn build_docx_entries(entries: &[(&str, &str)]) -> Vec<u8> {
    let mut out = Vec::new();
    let cursor = std::io::Cursor::new(&mut out);
    let mut zip = ZipWriter::new(cursor);
    let options = FileOptions::<()>::default();

    for (name, contents) in entries {
        zip.start_file(*name, options).expect("start zip entry");
        zip.write_all(contents.as_bytes()).expect("write zip entry");
    }

    zip.finish().expect("finish docx");
    out
}

pub(crate) fn build_minimal_docx(document_xml: &str) -> Vec<u8> {
    build_docx_entries(&[("word/document.xml", document_xml)])
}

pub(crate) fn sample_clean_session(
    id: &str,
    document_path: &str,
    source_text: &str,
) -> DocumentSession {
    let now = Utc::now();
    let mut session = DocumentSession {
        id: id.to_string(),
        title: "示例".to_string(),
        document_path: document_path.to_string(),
        source_text: source_text.to_string(),
        source_snapshot: None,
        template_kind: None,
        template_signature: None,
        slot_structure_signature: None,
        template_snapshot: None,
        normalized_text: source_text.to_string(),
        capabilities: DocumentSessionCapabilities {
            source_writeback: CapabilityGate::allowed(),
            editor_writeback: CapabilityGate::allowed(),
            ..Default::default()
        },
        segmentation_preset: Some(SegmentationPreset::Paragraph),
        rewrite_headings: Some(false),
        writeback_slots: Vec::new(),
        rewrite_units: Vec::new(),
        suggestions: Vec::new(),
        next_suggestion_sequence: 1,
        status: RunningState::Idle,
        created_at: now,
        updated_at: now,
    };
    crate::documents::hydrate_session_capabilities(&mut session);
    session
}

pub(crate) fn editable_slot(id: &str, order: usize, text: &str) -> WritebackSlot {
    WritebackSlot {
        id: id.to_string(),
        order,
        text: text.to_string(),
        editable: true,
        role: crate::rewrite_unit::WritebackSlotRole::EditableText,
        presentation: None,
        anchor: None,
        separator_after: String::new(),
    }
}

pub(crate) fn locked_slot(id: &str, order: usize, text: &str) -> WritebackSlot {
    WritebackSlot {
        id: id.to_string(),
        order,
        text: text.to_string(),
        editable: false,
        role: crate::rewrite_unit::WritebackSlotRole::LockedText,
        presentation: None,
        anchor: None,
        separator_after: String::new(),
    }
}

pub(crate) fn rewrite_unit(
    id: &str,
    order: usize,
    slot_ids: &[&str],
    display_text: &str,
    status: RewriteUnitStatus,
) -> RewriteUnit {
    RewriteUnit {
        id: id.to_string(),
        order,
        slot_ids: slot_ids.iter().map(|slot_id| slot_id.to_string()).collect(),
        display_text: display_text.to_string(),
        segmentation_preset: SegmentationPreset::Paragraph,
        status,
        error_message: None,
    }
}

pub(crate) fn rewrite_suggestion(
    id: &str,
    sequence: u64,
    rewrite_unit_id: &str,
    before_text: &str,
    after_text: &str,
    decision: SuggestionDecision,
    slot_updates: Vec<SlotUpdate>,
) -> RewriteSuggestion {
    let now = Utc::now();
    RewriteSuggestion {
        id: id.to_string(),
        sequence,
        rewrite_unit_id: rewrite_unit_id.to_string(),
        before_text: before_text.to_string(),
        after_text: after_text.to_string(),
        diff: DiffResult::default(),
        decision,
        slot_updates,
        created_at: now,
        updated_at: now,
    }
}
