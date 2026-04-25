use std::{fs, path::Path};

use tauri::AppHandle;

use crate::{
    document_snapshot::{capture_document_snapshot, SNAPSHOT_MISMATCH_ERROR},
    documents::{load_document_source, LoadedDocumentSource},
    models::{DocumentSession, DocumentSnapshot, SegmentationPreset},
    rewrite_unit::{build_rewrite_units, RewriteUnit, WritebackSlot},
    session_builder::{build_clean_session, CleanSessionBuildInput},
    storage,
};

#[path = "session_refresh/capabilities.rs"]
mod capabilities;
#[path = "session_refresh/draft.rs"]
mod draft;
#[path = "session_refresh/rules.rs"]
mod rules;

use capabilities::SessionCapabilities;
use draft::SessionRefreshDraft;
use rules::{
    decide_segmentation_refresh, session_can_rebuild_cleanly, source_snapshot_changed,
    SegmentationRefreshAction,
};

pub(crate) struct RefreshedSession {
    pub session: DocumentSession,
    pub changed: bool,
}

pub(super) struct SessionStructureData {
    pub(super) writeback_slots: Vec<WritebackSlot>,
    pub(super) rewrite_units: Vec<RewriteUnit>,
    pub(super) template_kind: Option<String>,
    pub(super) template_signature: Option<String>,
    pub(super) slot_structure_signature: Option<String>,
    pub(super) template_snapshot: Option<crate::textual_template::TextTemplate>,
    pub(super) segmentation_preset: SegmentationPreset,
    pub(super) rewrite_headings: bool,
}

impl SessionStructureData {
    fn from_loaded(
        loaded: &LoadedDocumentSource,
        segmentation_preset: SegmentationPreset,
        rewrite_headings: bool,
    ) -> Self {
        Self {
            writeback_slots: loaded.writeback_slots.clone(),
            rewrite_units: build_rewrite_units(&loaded.writeback_slots, segmentation_preset),
            template_kind: loaded.template_kind.clone(),
            template_signature: loaded.template_signature.clone(),
            slot_structure_signature: loaded.slot_structure_signature.clone(),
            template_snapshot: loaded.template_snapshot.clone(),
            segmentation_preset,
            rewrite_headings,
        }
    }
}

const SESSION_STRUCTURE_MISMATCH_ERROR: &str =
    "当前会话与当前版本的分块结构不一致，无法安全继续写回。请先重置记录后再继续。";

pub(crate) fn refresh_session_from_disk(
    app: &AppHandle,
    existing: &DocumentSession,
) -> Result<RefreshedSession, String> {
    let canonical = fs::canonicalize(&existing.document_path)
        .map_err(|error| format!("无法打开文件（路径无效或文件不存在）：{error}"))?;
    let settings = storage::load_settings(app)?;
    let loaded = load_document_source(&canonical, settings.rewrite_headings)?;
    let snapshot = capture_document_snapshot(&canonical).map(Some)?;
    Ok(refresh_session_from_loaded(
        existing,
        &canonical,
        loaded,
        settings.segmentation_preset,
        settings.rewrite_headings,
        snapshot,
    ))
}

fn refresh_session_from_loaded(
    existing: &DocumentSession,
    canonical: &Path,
    loaded: LoadedDocumentSource,
    segmentation_preset: SegmentationPreset,
    rewrite_headings: bool,
    current_snapshot: Option<DocumentSnapshot>,
) -> RefreshedSession {
    let capabilities = SessionCapabilities::from_loaded(&loaded);
    let source_changed = loaded.source_text != existing.source_text;
    let snapshot_changed = source_snapshot_changed(existing, current_snapshot.as_ref());

    if source_changed || snapshot_changed {
        return refresh_session_for_external_change(
            existing,
            canonical,
            loaded,
            segmentation_preset,
            rewrite_headings,
            current_snapshot,
        );
    }

    let mut draft = SessionRefreshDraft::new(existing.clone());
    draft.sync_document_path(canonical);

    let structure =
        SessionStructureData::from_loaded(&loaded, segmentation_preset, rewrite_headings);

    match decide_segmentation_refresh(
        &draft.session,
        rules::SegmentationRefreshExpectation {
            expected_template_kind: structure.template_kind.as_deref(),
            expected_template_signature: structure.template_signature.as_deref(),
            expected_slot_structure_signature: structure.slot_structure_signature.as_deref(),
            expected_writeback_slots: &structure.writeback_slots,
            expected_rewrite_units: &structure.rewrite_units,
            segmentation_preset: structure.segmentation_preset,
            rewrite_headings: structure.rewrite_headings,
        },
    ) {
        SegmentationRefreshAction::Keep => {
            draft.sync_template_metadata(
                structure.template_kind.clone(),
                structure.template_snapshot.clone(),
            );
        }
        SegmentationRefreshAction::Rebuild => {
            draft.rebuild_structure(structure);
        }
        SegmentationRefreshAction::Block => {
            return block_session_for_structure_change(existing, canonical);
        }
    }

    draft.apply_capabilities(&capabilities);
    draft.finish()
}

fn refresh_session_for_external_change(
    existing: &DocumentSession,
    canonical: &Path,
    loaded: LoadedDocumentSource,
    segmentation_preset: SegmentationPreset,
    rewrite_headings: bool,
    current_snapshot: Option<DocumentSnapshot>,
) -> RefreshedSession {
    if session_can_rebuild_cleanly(existing) {
        return RefreshedSession {
            session: build_clean_session(CleanSessionBuildInput {
                session_id: existing.id.clone(),
                canonical_path: canonical,
                document_path: canonical.to_string_lossy().to_string(),
                loaded,
                source_snapshot: current_snapshot,
                segmentation_preset,
                rewrite_headings,
                created_at: existing.created_at,
            }),
            changed: true,
        };
    }
    block_session_for_external_change(existing, canonical)
}

fn block_session_for_external_change(
    existing: &DocumentSession,
    canonical: &Path,
) -> RefreshedSession {
    block_session_with_reason(existing, canonical, SNAPSHOT_MISMATCH_ERROR)
}

fn block_session_for_structure_change(
    existing: &DocumentSession,
    canonical: &Path,
) -> RefreshedSession {
    block_session_with_reason(existing, canonical, SESSION_STRUCTURE_MISMATCH_ERROR)
}

fn block_session_with_reason(
    existing: &DocumentSession,
    canonical: &Path,
    reason: &str,
) -> RefreshedSession {
    let mut draft = SessionRefreshDraft::new(existing.clone());
    draft.sync_document_path(canonical);
    draft.apply_capabilities(&SessionCapabilities::blocked(reason));
    draft.finish()
}

#[cfg(test)]
#[path = "session_refresh/refresh_change_tests.rs"]
mod refresh_change_tests;
#[cfg(test)]
#[path = "session_refresh/refresh_structure_tests.rs"]
mod refresh_structure_tests;
#[cfg(test)]
#[path = "session_refresh/test_support.rs"]
mod test_support;
