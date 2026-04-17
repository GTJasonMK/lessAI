use std::path::Path;

use chrono::Utc;

use crate::{
    models::{SegmentationPreset, DocumentSession, RunningState},
    rewrite,
    rewrite_unit::{RewriteUnit, WritebackSlot},
};

use super::{
    capabilities::{apply_session_capabilities, SessionCapabilities},
    RefreshedSession,
};

pub(super) struct SessionRefreshDraft {
    pub(super) session: DocumentSession,
    changed: bool,
}

impl SessionRefreshDraft {
    pub(super) fn new(session: DocumentSession) -> Self {
        Self {
            session,
            changed: false,
        }
    }

    pub(super) fn sync_document_path(&mut self, canonical: &Path) {
        let canonical_path = canonical.to_string_lossy().to_string();
        if self.session.document_path == canonical_path {
            return;
        }
        self.session.document_path = canonical_path;
        self.changed = true;
    }

    pub(super) fn rebuild_structure(
        &mut self,
        writeback_slots: Vec<WritebackSlot>,
        rewrite_units: Vec<RewriteUnit>,
        segmentation_preset: SegmentationPreset,
        rewrite_headings: bool,
    ) {
        self.session.normalized_text = rewrite::normalize_text(&self.session.source_text);
        self.session.writeback_slots = writeback_slots;
        self.session.rewrite_units = rewrite_units;
        self.session.segmentation_preset = Some(segmentation_preset);
        self.session.rewrite_headings = Some(rewrite_headings);
        self.session.status = RunningState::Idle;
        self.changed = true;
    }

    pub(super) fn apply_capabilities(&mut self, capabilities: &SessionCapabilities) {
        if apply_session_capabilities(&mut self.session, capabilities) {
            self.changed = true;
        }
    }

    pub(super) fn finish(mut self) -> RefreshedSession {
        if self.changed {
            self.session.updated_at = Utc::now();
        }
        RefreshedSession {
            session: self.session,
            changed: self.changed,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use chrono::Duration;

    use super::SessionRefreshDraft;
    use crate::{
        models::{SegmentationPreset, RewriteUnitStatus, RunningState},
        rewrite_unit::{RewriteUnit, WritebackSlot},
        session_refresh::test_support::sample_session,
    };

    #[test]
    fn refresh_draft_updates_timestamp_only_after_real_change() {
        let mut session = sample_session();
        session.updated_at -= Duration::seconds(1);
        let original_updated_at = session.updated_at;

        let unchanged = SessionRefreshDraft::new(session.clone()).finish();
        assert!(!unchanged.changed);
        assert_eq!(unchanged.session.updated_at, original_updated_at);

        let mut changed = SessionRefreshDraft::new(session);
        changed.sync_document_path(Path::new("/tmp/canonical/example.docx"));
        let changed = changed.finish();
        assert!(changed.changed);
        assert_eq!(changed.session.document_path, "/tmp/canonical/example.docx");
        assert!(changed.session.updated_at > original_updated_at);
    }

    #[test]
    fn refresh_draft_rebuild_structure_resets_session_metadata_in_one_place() {
        let mut session = sample_session();
        session.source_text = "第一句。第二句。".to_string();
        session.status = RunningState::Completed;
        session.segmentation_preset = Some(SegmentationPreset::Paragraph);
        session.rewrite_headings = Some(false);

        let mut draft = SessionRefreshDraft::new(session);
        draft.rebuild_structure(
            vec![WritebackSlot::editable("slot-0", 0, "第一句。第二句。")],
            vec![RewriteUnit {
                id: "unit-0".to_string(),
                order: 0,
                slot_ids: vec!["slot-0".to_string()],
                display_text: "第一句。第二句。".to_string(),
                segmentation_preset: SegmentationPreset::Sentence,
                status: RewriteUnitStatus::Idle,
                error_message: None,
            }],
            SegmentationPreset::Sentence,
            true,
        );
        let refreshed = draft.finish();

        assert!(refreshed.changed);
        assert_eq!(refreshed.session.normalized_text, "第一句。第二句。");
        assert_eq!(refreshed.session.writeback_slots.len(), 1);
        assert_eq!(refreshed.session.rewrite_units.len(), 1);
        assert_eq!(refreshed.session.segmentation_preset, Some(SegmentationPreset::Sentence));
        assert_eq!(refreshed.session.rewrite_headings, Some(true));
        assert_eq!(refreshed.session.status, RunningState::Idle);
    }
}
