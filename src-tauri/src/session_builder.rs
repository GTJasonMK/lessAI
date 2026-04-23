use std::path::Path;

use chrono::{DateTime, Utc};

use crate::{
    documents::{hydrate_session_capabilities, LoadedDocumentSource},
    models::{DocumentSession, DocumentSnapshot, RunningState, SegmentationPreset},
    rewrite_unit::build_rewrite_units,
    session_capability_models::DocumentSessionCapabilities,
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
        template_kind,
        template_signature,
        slot_structure_signature,
        template_snapshot,
        writeback_slots,
        capability_policy,
    } = input.loaded;
    let normalized_text = crate::rewrite::normalize_text(&source_text);
    let rewrite_units = build_rewrite_units(&writeback_slots, input.segmentation_preset);
    let now = Utc::now();

    let mut session = DocumentSession {
        id: input.session_id,
        title: session_title(input.canonical_path),
        document_path: input.document_path,
        source_text,
        source_snapshot: input.source_snapshot,
        template_kind,
        template_signature,
        slot_structure_signature,
        template_snapshot,
        normalized_text,
        capabilities: DocumentSessionCapabilities {
            source_writeback: capability_policy.source_writeback,
            editor_writeback: capability_policy.editor_writeback,
            ..Default::default()
        },
        segmentation_preset: Some(input.segmentation_preset),
        rewrite_headings: Some(input.rewrite_headings),
        writeback_slots,
        rewrite_units,
        suggestions: Vec::new(),
        next_suggestion_sequence: 1,
        status: RunningState::Idle,
        created_at: input.created_at,
        updated_at: now,
    };
    hydrate_session_capabilities(&mut session);
    session
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
        models::{DocumentSnapshot, RunningState, SegmentationPreset},
        rewrite_unit::WritebackSlot,
    };

    #[test]
    fn build_clean_session_reuses_loaded_capabilities_and_segmentation_settings() {
        let created_at = Utc::now();
        let loaded = LoadedDocumentSource {
            source_text: "前文[公式]后文".to_string(),
            template_kind: None,
            template_signature: None,
            slot_structure_signature: None,
            template_snapshot: None,
            writeback_slots: vec![
                WritebackSlot::editable("slot-0", 0, "前文"),
                WritebackSlot::locked("slot-1", 1, "[公式]"),
                WritebackSlot::editable("slot-2", 2, "后文"),
            ],
            capability_policy: crate::documents::DocumentCapabilityPolicy::new(
                crate::documents::capability_gate(false, Some("blocked")),
                crate::documents::capability_gate(false, Some("editor blocked")),
            ),
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
        assert_eq!(
            session.segmentation_preset,
            Some(SegmentationPreset::Paragraph)
        );
        assert_eq!(session.rewrite_headings, Some(true));
        assert_eq!(session.writeback_slots.len(), 3);
        assert_eq!(session.rewrite_units.len(), 1);
        assert_eq!(
            session.rewrite_units[0].slot_ids,
            vec!["slot-0", "slot-1", "slot-2"]
        );
        assert!(!session.capabilities.source_writeback.allowed);
        assert_eq!(
            session.capabilities.source_writeback.block_reason.as_deref(),
            Some("blocked")
        );
        assert!(!session.capabilities.editor_writeback.allowed);
        assert_eq!(
            session
                .capabilities
                .editor_writeback
                .block_reason
                .as_deref(),
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
            template_kind: None,
            template_signature: None,
            slot_structure_signature: None,
            template_snapshot: None,
            writeback_slots: vec![
                WritebackSlot::editable("slot-1", 0, "甲"),
                WritebackSlot::editable("slot-2", 1, "乙"),
            ],
            capability_policy: crate::documents::DocumentCapabilityPolicy::new(
                crate::documents::capability_gate(true, None),
                crate::documents::capability_gate(true, None),
            ),
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

    #[test]
    fn build_clean_session_persists_textual_template_metadata() {
        let template = crate::textual_template::models::TextTemplate::single_paragraph(
            "plain_text",
            "txt:p0",
            "第一段\n\n",
        );
        let built = crate::textual_template::slots::build_slots(&template);
        let loaded = LoadedDocumentSource {
            source_text: "第一段\n\n".to_string(),
            template_kind: Some("plain_text".to_string()),
            template_signature: Some(template.template_signature.clone()),
            slot_structure_signature: Some(built.slot_structure_signature.clone()),
            template_snapshot: Some(template.clone()),
            writeback_slots: built.slots,
            capability_policy: crate::documents::DocumentCapabilityPolicy::new(
                crate::documents::capability_gate(true, None),
                crate::documents::capability_gate(true, None),
            ),
        };

        let session = super::build_clean_session(super::CleanSessionBuildInput {
            session_id: "session-1".to_string(),
            canonical_path: std::path::Path::new("/tmp/example.txt"),
            document_path: "/tmp/example.txt".to_string(),
            loaded,
            source_snapshot: None,
            segmentation_preset: SegmentationPreset::Paragraph,
            rewrite_headings: false,
            created_at: Utc::now(),
        });

        assert_eq!(session.template_kind.as_deref(), Some("plain_text"));
        assert!(session.template_snapshot.is_some());
        assert!(session.template_signature.is_some());
        assert!(session.slot_structure_signature.is_some());
    }
}
