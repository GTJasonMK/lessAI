use std::path::Path;

use chrono::{DateTime, Utc};
use tauri::AppHandle;

use crate::{
    document_snapshot::capture_document_snapshot,
    documents::load_document_source,
    models::DocumentSession,
    session_builder::{build_clean_session, CleanSessionBuildInput},
    storage,
};

pub(crate) struct LoadedCleanSessionBuildInput<'a> {
    pub session_id: String,
    pub canonical_path: &'a Path,
    pub document_path: String,
    pub loaded: crate::documents::LoadedDocumentSource,
    pub source_snapshot: Option<crate::models::DocumentSnapshot>,
    pub segmentation_preset: crate::models::SegmentationPreset,
    pub rewrite_headings: bool,
    pub created_at: DateTime<Utc>,
    pub reject_empty: bool,
}

pub(crate) struct DiskCleanSessionLoadInput<'a> {
    pub session_id: String,
    pub canonical_path: &'a Path,
    pub document_path: String,
    pub created_at: DateTime<Utc>,
    pub reject_empty: bool,
}

pub(crate) fn load_clean_session_from_existing(
    app: &AppHandle,
    existing: &DocumentSession,
    created_at: DateTime<Utc>,
    reject_empty: bool,
) -> Result<DocumentSession, String> {
    load_clean_session_from_disk(
        app,
        disk_clean_session_load_input_for_existing(existing, created_at, reject_empty),
    )
}

fn disk_clean_session_load_input_for_existing<'a>(
    existing: &'a DocumentSession,
    created_at: DateTime<Utc>,
    reject_empty: bool,
) -> DiskCleanSessionLoadInput<'a> {
    DiskCleanSessionLoadInput {
        session_id: existing.id.clone(),
        canonical_path: Path::new(&existing.document_path),
        document_path: existing.document_path.clone(),
        created_at,
        reject_empty,
    }
}

pub(crate) fn build_loaded_clean_session(
    input: LoadedCleanSessionBuildInput<'_>,
) -> Result<DocumentSession, String> {
    if input.reject_empty && input.loaded.source_text.trim().is_empty() {
        return Err("文档内容为空。".to_string());
    }

    Ok(build_clean_session(CleanSessionBuildInput {
        session_id: input.session_id,
        canonical_path: input.canonical_path,
        document_path: input.document_path,
        loaded: input.loaded,
        source_snapshot: input.source_snapshot,
        segmentation_preset: input.segmentation_preset,
        rewrite_headings: input.rewrite_headings,
        created_at: input.created_at,
    }))
}

pub(crate) fn load_clean_session_from_disk(
    app: &AppHandle,
    input: DiskCleanSessionLoadInput<'_>,
) -> Result<DocumentSession, String> {
    let settings = storage::load_settings(app)?;
    let loaded = load_document_source(input.canonical_path, settings.rewrite_headings)?;
    let source_snapshot = Some(capture_document_snapshot(input.canonical_path)?);

    build_loaded_clean_session(LoadedCleanSessionBuildInput {
        session_id: input.session_id,
        canonical_path: input.canonical_path,
        document_path: input.document_path,
        loaded,
        source_snapshot,
        segmentation_preset: settings.segmentation_preset,
        rewrite_headings: settings.rewrite_headings,
        created_at: input.created_at,
        reject_empty: input.reject_empty,
    })
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use crate::{
        documents::LoadedDocumentSource,
        models::{DocumentSession, DocumentSnapshot, RunningState, SegmentationPreset},
        rewrite_unit::WritebackSlot,
    };

    fn sample_loaded(source_text: &str) -> LoadedDocumentSource {
        LoadedDocumentSource {
            source_text: source_text.to_string(),
            template_kind: None,
            template_signature: None,
            slot_structure_signature: None,
            template_snapshot: None,
            writeback_slots: vec![WritebackSlot::editable("slot-0", 0, source_text)],
            capability_policy: crate::documents::DocumentCapabilityPolicy::new(
                crate::documents::capability_gate(true, None),
                crate::documents::capability_gate(true, None),
            ),
        }
    }

    #[test]
    fn build_loaded_clean_session_rejects_empty_source_when_requested() {
        let error = super::build_loaded_clean_session(super::LoadedCleanSessionBuildInput {
            session_id: "session-1".to_string(),
            canonical_path: std::path::Path::new("/tmp/example.txt"),
            document_path: "/tmp/example.txt".to_string(),
            loaded: sample_loaded("   "),
            source_snapshot: Some(DocumentSnapshot {
                sha256: "snap".to_string(),
            }),
            segmentation_preset: SegmentationPreset::Paragraph,
            rewrite_headings: false,
            created_at: Utc::now(),
            reject_empty: true,
        })
        .expect_err("expected empty source to be rejected");

        assert_eq!(error, "文档内容为空。");
    }

    #[test]
    fn build_loaded_clean_session_allows_empty_source_when_not_requested() {
        let session = super::build_loaded_clean_session(super::LoadedCleanSessionBuildInput {
            session_id: "session-2".to_string(),
            canonical_path: std::path::Path::new("/tmp/example.txt"),
            document_path: "/tmp/example.txt".to_string(),
            loaded: sample_loaded("   "),
            source_snapshot: Some(DocumentSnapshot {
                sha256: "snap".to_string(),
            }),
            segmentation_preset: SegmentationPreset::Paragraph,
            rewrite_headings: false,
            created_at: Utc::now(),
            reject_empty: false,
        })
        .expect("expected empty source to be allowed when reject_empty=false");

        assert_eq!(session.source_text, "   ");
        assert_eq!(session.status, RunningState::Idle);
    }

    #[test]
    fn disk_clean_session_load_input_for_existing_reuses_existing_identity_and_path() {
        let now = Utc::now();
        let existing = DocumentSession {
            id: "session-9".to_string(),
            title: "示例".to_string(),
            document_path: "/tmp/example.txt".to_string(),
            source_text: "正文".to_string(),
            source_snapshot: None,
            template_kind: None,
            template_signature: None,
            slot_structure_signature: None,
            template_snapshot: None,
            normalized_text: "正文".to_string(),
            capabilities: crate::session_capability_models::DocumentSessionCapabilities {
                source_writeback: crate::session_capability_models::CapabilityGate::allowed(),
                editor_writeback: crate::session_capability_models::CapabilityGate::allowed(),
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

        let input = super::disk_clean_session_load_input_for_existing(&existing, now, true);

        assert_eq!(input.session_id, "session-9");
        assert_eq!(
            input.canonical_path,
            std::path::Path::new("/tmp/example.txt")
        );
        assert_eq!(input.document_path, "/tmp/example.txt");
        assert_eq!(input.created_at, now);
        assert!(input.reject_empty);
    }
}
