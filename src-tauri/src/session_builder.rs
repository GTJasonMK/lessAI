use std::path::Path;

use chrono::{DateTime, Utc};

use crate::{
    documents::LoadedDocumentSource,
    models::{SegmentationPreset, DocumentSession, DocumentSnapshot, RunningState},
    rewrite_unit::build_rewrite_units,
};

pub(crate) struct CleanSessionBuildInput<'a> {
    pub session_id: String,
    pub canonical_path: &'a Path,
    pub document_path: String,
    pub loaded: LoadedDocumentSource,
    pub source_snapshot: Option<DocumentSnapshot>,
    pub segmentation_preset: SegmentationPreset,
    pub rewrite_headings: bool,
    pub created_at: DateTime<Utc>,
}

pub(crate) fn build_clean_session(input: CleanSessionBuildInput<'_>) -> DocumentSession {
    let LoadedDocumentSource {
        source_text,
        writeback_slots,
        write_back_supported,
        write_back_block_reason,
        plain_text_editor_safe,
        plain_text_editor_block_reason,
    } = input.loaded;
    let normalized_text = crate::rewrite::normalize_text(&source_text);
    let rewrite_units = build_rewrite_units(&writeback_slots, input.segmentation_preset);
    let now = Utc::now();

    DocumentSession {
        id: input.session_id,
        title: session_title(input.canonical_path),
        document_path: input.document_path,
        source_text,
        source_snapshot: input.source_snapshot,
        normalized_text,
        write_back_supported,
        write_back_block_reason,
        plain_text_editor_safe,
        plain_text_editor_block_reason,
        segmentation_preset: Some(input.segmentation_preset),
        rewrite_headings: Some(input.rewrite_headings),
        writeback_slots,
        rewrite_units,
        suggestions: Vec::new(),
        next_suggestion_sequence: 1,
        status: RunningState::Idle,
        created_at: input.created_at,
        updated_at: now,
    }
}

fn session_title(path: &Path) -> String {
    path.file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("未命名文稿")
        .to_string()
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use crate::{
        documents::LoadedDocumentSource,
        models::{SegmentationPreset, DocumentSnapshot, RunningState},
        rewrite_unit::WritebackSlot,
    };

    #[test]
    fn build_clean_session_reuses_loaded_capabilities_and_segmentation_settings() {
        let created_at = Utc::now();
        let loaded = LoadedDocumentSource {
            source_text: "前文[公式]后文".to_string(),
            writeback_slots: vec![
                WritebackSlot::editable("slot-0", 0, "前文"),
                WritebackSlot::locked("slot-1", 1, "[公式]"),
                WritebackSlot::editable("slot-2", 2, "后文"),
            ],
            write_back_supported: false,
            write_back_block_reason: Some("blocked".to_string()),
            plain_text_editor_safe: false,
            plain_text_editor_block_reason: Some("editor blocked".to_string()),
        };

        let session = super::build_clean_session(super::CleanSessionBuildInput {
            session_id: "session-1".to_string(),
            canonical_path: std::path::Path::new("/tmp/renamed.docx"),
            document_path: "/tmp/renamed.docx".to_string(),
            loaded,
            source_snapshot: Some(DocumentSnapshot {
                sha256: "new".to_string(),
            }),
            segmentation_preset: SegmentationPreset::Paragraph,
            rewrite_headings: true,
            created_at,
        });

        assert_eq!(session.document_path, "/tmp/renamed.docx");
        assert_eq!(session.title, "renamed");
        assert_eq!(
            session
                .source_snapshot
                .as_ref()
                .map(|item| item.sha256.as_str()),
            Some("new")
        );
        assert_eq!(session.segmentation_preset, Some(SegmentationPreset::Paragraph));
        assert_eq!(session.rewrite_headings, Some(true));
        assert_eq!(session.writeback_slots.len(), 3);
        assert_eq!(session.rewrite_units.len(), 1);
        assert_eq!(
            session.rewrite_units[0].slot_ids,
            vec!["slot-0", "slot-1", "slot-2"]
        );
        assert!(!session.write_back_supported);
        assert_eq!(session.write_back_block_reason.as_deref(), Some("blocked"));
        assert!(!session.plain_text_editor_safe);
        assert_eq!(
            session.plain_text_editor_block_reason.as_deref(),
            Some("editor blocked")
        );
        assert_eq!(session.created_at, created_at);
        assert_eq!(session.next_suggestion_sequence, 1);
        assert!(session.suggestions.is_empty());
        assert_eq!(session.status, RunningState::Idle);
    }

    #[test]
    fn build_clean_session_stores_writeback_slots_and_rewrite_units() {
        let loaded = LoadedDocumentSource {
            source_text: "甲乙".to_string(),
            writeback_slots: vec![
                WritebackSlot::editable("slot-1", 0, "甲"),
                WritebackSlot::editable("slot-2", 1, "乙"),
            ],
            write_back_supported: true,
            write_back_block_reason: None,
            plain_text_editor_safe: true,
            plain_text_editor_block_reason: None,
        };

        let session = super::build_clean_session(super::CleanSessionBuildInput {
            session_id: "session-1".to_string(),
            canonical_path: std::path::Path::new("/tmp/example.txt"),
            document_path: "/tmp/example.txt".to_string(),
            loaded,
            source_snapshot: None,
            segmentation_preset: SegmentationPreset::Sentence,
            rewrite_headings: false,
            created_at: Utc::now(),
        });

        assert_eq!(session.writeback_slots.len(), 2);
        assert_eq!(session.rewrite_units.len(), 1);
        assert_eq!(session.rewrite_units[0].slot_ids, vec!["slot-1", "slot-2"]);
    }
}
