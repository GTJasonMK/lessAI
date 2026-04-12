use std::{
    fs,
    path::{Path, PathBuf},
};

use chrono::Utc;
use tauri::AppHandle;

use crate::{
    document_snapshot::capture_document_snapshot,
    documents::{
        detect_document_capabilities, document_format, load_document_source, LoadedDocumentSource,
    },
    models::{
        ChunkPreset, ChunkStatus, ChunkTask, DocumentSession, DocumentSnapshot, RunningState,
    },
    rewrite,
    state::{with_session_lock, AppState},
    storage,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SnapshotRepairOutcome {
    None,
    Backfilled,
    Rebuilt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SnapshotRepairAction {
    None,
    Backfill,
    Rebuild,
}

pub(crate) fn repair_session_snapshot_if_needed(
    app: &AppHandle,
    session: &mut DocumentSession,
) -> Result<SnapshotRepairOutcome, String> {
    if session.source_snapshot.is_some() {
        return Ok(SnapshotRepairOutcome::None);
    }

    let canonical = canonical_document_path(&session.document_path)?;
    let settings = storage::load_settings(app)?;
    let loaded = load_document_source(&canonical, settings.rewrite_headings)?;
    let action = snapshot_repair_action(session, &loaded.source_text);
    if action == SnapshotRepairAction::None {
        return Ok(SnapshotRepairOutcome::None);
    }

    let snapshot = capture_document_snapshot(&canonical)?;
    Ok(apply_snapshot_repair(
        session,
        &canonical,
        loaded,
        snapshot,
        settings.chunk_preset,
        settings.rewrite_headings,
        action,
    ))
}

pub(crate) fn rebuild_session_from_current_document(
    existing: &DocumentSession,
    canonical: &Path,
    loaded: LoadedDocumentSource,
    chunk_preset: ChunkPreset,
    rewrite_headings: bool,
) -> Result<DocumentSession, String> {
    let snapshot = capture_document_snapshot(canonical)?;
    Ok(rebuild_session_from_loaded(
        existing,
        canonical,
        loaded,
        snapshot,
        chunk_preset,
        rewrite_headings,
    ))
}

pub(crate) fn load_session_with_snapshot_repairs(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
) -> Result<DocumentSession, String> {
    with_session_lock(state, session_id, || {
        let mut session = storage::load_session(app, session_id)?;
        let repaired = repair_session_snapshot_if_needed(app, &mut session)?;
        let refreshed = refresh_session_capabilities_if_needed(&mut session)?;
        if repaired != SnapshotRepairOutcome::None || refreshed {
            storage::save_session(app, &session)?;
        }
        Ok(session)
    })
}

pub(crate) fn refresh_session_capabilities_if_needed(
    session: &mut DocumentSession,
) -> Result<bool, String> {
    let canonical = canonical_document_path(&session.document_path)?;
    let canonical_path = canonical.to_string_lossy().to_string();
    let (
        write_back_supported,
        write_back_block_reason,
        plain_text_editor_safe,
        plain_text_editor_block_reason,
    ) = detect_document_capabilities(&canonical)?;
    let mut changed = false;

    if session.document_path != canonical_path {
        session.document_path = canonical_path;
        changed = true;
    }
    if session.write_back_supported != write_back_supported
        || session.write_back_block_reason != write_back_block_reason
        || session.plain_text_editor_safe != plain_text_editor_safe
        || session.plain_text_editor_block_reason != plain_text_editor_block_reason
    {
        session.write_back_supported = write_back_supported;
        session.write_back_block_reason = write_back_block_reason;
        session.plain_text_editor_safe = plain_text_editor_safe;
        session.plain_text_editor_block_reason = plain_text_editor_block_reason;
        changed = true;
    }

    if changed {
        session.updated_at = Utc::now();
    }

    Ok(changed)
}

fn apply_snapshot_repair(
    session: &mut DocumentSession,
    canonical: &Path,
    loaded: LoadedDocumentSource,
    snapshot: DocumentSnapshot,
    chunk_preset: ChunkPreset,
    rewrite_headings: bool,
    action: SnapshotRepairAction,
) -> SnapshotRepairOutcome {
    match action {
        SnapshotRepairAction::None => SnapshotRepairOutcome::None,
        SnapshotRepairAction::Backfill => {
            session.source_snapshot = Some(snapshot);
            session.updated_at = Utc::now();
            SnapshotRepairOutcome::Backfilled
        }
        SnapshotRepairAction::Rebuild => {
            *session = rebuild_session_from_loaded(
                session,
                canonical,
                loaded,
                snapshot,
                chunk_preset,
                rewrite_headings,
            );
            SnapshotRepairOutcome::Rebuilt
        }
    }
}

fn rebuild_session_from_loaded(
    existing: &DocumentSession,
    canonical: &Path,
    loaded: LoadedDocumentSource,
    snapshot: DocumentSnapshot,
    chunk_preset: ChunkPreset,
    rewrite_headings: bool,
) -> DocumentSession {
    let LoadedDocumentSource {
        source_text,
        regions,
        region_segmentation_strategy,
        write_back_supported,
        write_back_block_reason,
        plain_text_editor_safe,
        plain_text_editor_block_reason,
    } = loaded;
    let now = Utc::now();

    DocumentSession {
        id: existing.id.clone(),
        title: title_from_path(canonical),
        document_path: canonical.to_string_lossy().to_string(),
        normalized_text: rewrite::normalize_text(&source_text),
        chunks: build_chunks(
            canonical,
            regions,
            region_segmentation_strategy,
            chunk_preset,
        ),
        source_text,
        source_snapshot: Some(snapshot),
        write_back_supported,
        write_back_block_reason,
        plain_text_editor_safe,
        plain_text_editor_block_reason,
        chunk_preset: Some(chunk_preset),
        rewrite_headings: Some(rewrite_headings),
        suggestions: Vec::new(),
        next_suggestion_sequence: 1,
        status: RunningState::Idle,
        created_at: existing.created_at,
        updated_at: now,
    }
}

fn build_chunks(
    path: &Path,
    regions: Vec<crate::adapters::TextRegion>,
    region_segmentation_strategy: crate::documents::RegionSegmentationStrategy,
    chunk_preset: ChunkPreset,
) -> Vec<ChunkTask> {
    rewrite::segment_regions_with_strategy(
        regions,
        chunk_preset,
        document_format(path),
        region_segmentation_strategy,
    )
    .into_iter()
    .enumerate()
    .map(|(index, chunk)| ChunkTask {
        index,
        source_text: chunk.text,
        separator_after: chunk.separator_after,
        skip_rewrite: chunk.skip_rewrite,
        presentation: chunk.presentation,
        status: if chunk.skip_rewrite {
            ChunkStatus::Done
        } else {
            ChunkStatus::Idle
        },
        error_message: None,
    })
    .collect()
}

fn snapshot_repair_action(
    session: &DocumentSession,
    loaded_source_text: &str,
) -> SnapshotRepairAction {
    if session.source_snapshot.is_some() {
        return SnapshotRepairAction::None;
    }
    if loaded_source_text == session.source_text {
        return SnapshotRepairAction::Backfill;
    }
    if can_rebuild_snapshotless_session(session) {
        return SnapshotRepairAction::Rebuild;
    }
    SnapshotRepairAction::None
}

fn can_rebuild_snapshotless_session(session: &DocumentSession) -> bool {
    session.status == RunningState::Idle
        && session.suggestions.is_empty()
        && session.chunks.iter().all(|chunk| {
            (chunk.skip_rewrite && chunk.status == ChunkStatus::Done)
                || (!chunk.skip_rewrite && chunk.status == ChunkStatus::Idle)
        })
}

fn canonical_document_path(document_path: &str) -> Result<PathBuf, String> {
    fs::canonicalize(document_path)
        .map_err(|error| format!("无法打开文件（路径无效或文件不存在）：{error}"))
}

fn title_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("未命名文稿")
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::{env, fs, path::PathBuf};

    use chrono::Utc;
    use uuid::Uuid;

    use super::{
        refresh_session_capabilities_if_needed, snapshot_repair_action, SnapshotRepairAction,
    };
    use crate::models::{ChunkStatus, ChunkTask, DocumentSession, DocumentSnapshot, RunningState};

    fn sample_session() -> DocumentSession {
        let now = Utc::now();
        DocumentSession {
            id: "session-1".to_string(),
            title: "示例".to_string(),
            document_path: "/tmp/example.docx".to_string(),
            source_text: "旧文本".to_string(),
            source_snapshot: None,
            normalized_text: "旧文本".to_string(),
            write_back_supported: true,
            write_back_block_reason: None,
            plain_text_editor_safe: true,
            plain_text_editor_block_reason: None,
            chunk_preset: Some(ChunkPreset::Paragraph),
            rewrite_headings: Some(false),
            chunks: vec![ChunkTask {
                index: 0,
                source_text: "旧文本".to_string(),
                separator_after: String::new(),
                skip_rewrite: false,
                presentation: None,
                status: ChunkStatus::Idle,
                error_message: None,
            }],
            suggestions: Vec::new(),
            next_suggestion_sequence: 1,
            status: RunningState::Idle,
            created_at: now,
            updated_at: now,
        }
    }

    fn unique_test_dir(name: &str) -> PathBuf {
        env::temp_dir().join(format!("lessai-session-repair-{name}-{}", Uuid::new_v4()))
    }

    #[test]
    fn backfills_snapshot_when_loaded_source_matches() {
        let session = sample_session();
        assert_eq!(
            snapshot_repair_action(&session, "旧文本"),
            SnapshotRepairAction::Backfill
        );
    }

    #[test]
    fn rebuilds_clean_snapshotless_session_when_source_changed() {
        let session = sample_session();
        assert_eq!(
            snapshot_repair_action(&session, "新文本"),
            SnapshotRepairAction::Rebuild
        );
    }

    #[test]
    fn keeps_explicit_error_for_dirty_snapshotless_session() {
        let mut session = sample_session();
        session.status = RunningState::Completed;
        assert_eq!(
            snapshot_repair_action(&session, "新文本"),
            SnapshotRepairAction::None
        );
    }

    #[test]
    fn ignores_sessions_that_already_have_snapshot() {
        let mut session = sample_session();
        session.source_snapshot = Some(DocumentSnapshot {
            sha256: "abc".to_string(),
        });
        assert_eq!(
            snapshot_repair_action(&session, "旧文本"),
            SnapshotRepairAction::None
        );
    }

    #[test]
    fn refreshes_stale_capabilities_from_current_file() {
        let root = unique_test_dir("capabilities");
        fs::create_dir_all(&root).expect("create root");
        let target = root.join("sample.txt");
        fs::write(&target, "正文").expect("write source");

        let mut session = sample_session();
        session.document_path = target.to_string_lossy().to_string();
        session.write_back_supported = false;
        session.write_back_block_reason = Some("旧的阻止原因".to_string());
        session.plain_text_editor_safe = false;
        session.plain_text_editor_block_reason = Some("旧的编辑阻止原因".to_string());

        let changed =
            refresh_session_capabilities_if_needed(&mut session).expect("refresh capabilities");

        assert!(changed);
        assert!(session.write_back_supported);
        assert_eq!(session.write_back_block_reason, None);
        assert!(session.plain_text_editor_safe);
        assert_eq!(session.plain_text_editor_block_reason, None);

        let _ = fs::remove_dir_all(root);
    }
}
