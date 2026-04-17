use std::{fs, path::Path};

use tauri::AppHandle;

use crate::{
    document_snapshot::{capture_document_snapshot, SNAPSHOT_MISMATCH_ERROR},
    documents::{load_document_source, LoadedDocumentSource},
    models::{SegmentationPreset, DocumentSession, DocumentSnapshot},
    session_builder::{build_clean_session, CleanSessionBuildInput},
    rewrite_unit::build_rewrite_units,
    storage,
};

mod capabilities;
mod draft;
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

    let expected_rewrite_units = build_rewrite_units(&loaded.writeback_slots, segmentation_preset);

    match decide_segmentation_refresh(
        &draft.session,
        &loaded.writeback_slots,
        &expected_rewrite_units,
        segmentation_preset,
        rewrite_headings,
    ) {
        SegmentationRefreshAction::Keep => {}
        SegmentationRefreshAction::Rebuild => {
            draft.rebuild_structure(
                loaded.writeback_slots,
                expected_rewrite_units,
                segmentation_preset,
                rewrite_headings,
            );
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
mod refresh_change_tests;
#[cfg(test)]
mod refresh_structure_tests;
#[cfg(test)]
mod test_support;
