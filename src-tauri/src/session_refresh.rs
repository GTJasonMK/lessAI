use std::{fs, path::Path};

use chrono::Utc;
use tauri::AppHandle;

use crate::{
    document_snapshot::capture_document_snapshot,
    documents::{document_format, load_document_source, LoadedDocumentSource},
    models::{
        ChunkPreset, ChunkStatus, ChunkTask, DocumentSession, DocumentSnapshot, RunningState,
    },
    rewrite,
    session_repair::rebuild_session_from_current_document,
    storage,
};

pub(crate) struct RefreshedSession {
    pub session: DocumentSession,
    pub changed: bool,
}

pub(crate) fn refresh_session_from_disk(
    app: &AppHandle,
    existing: &DocumentSession,
) -> Result<RefreshedSession, String> {
    let canonical = fs::canonicalize(&existing.document_path)
        .map_err(|error| format!("无法打开文件（路径无效或文件不存在）：{error}"))?;
    let settings = storage::load_settings(app)?;
    let loaded = load_document_source(&canonical, settings.rewrite_headings)?;

    if loaded.source_text != existing.source_text {
        let session = rebuild_session_from_current_document(
            existing,
            &canonical,
            loaded,
            settings.chunk_preset,
            settings.rewrite_headings,
        )?;
        return Ok(RefreshedSession {
            session,
            changed: true,
        });
    }

    let snapshot = missing_snapshot(&canonical, existing)?;
    Ok(refresh_session_from_loaded(
        existing,
        &canonical,
        loaded,
        settings.chunk_preset,
        settings.rewrite_headings,
        snapshot,
    ))
}

fn missing_snapshot(
    canonical: &Path,
    existing: &DocumentSession,
) -> Result<Option<DocumentSnapshot>, String> {
    if existing.source_snapshot.is_some() {
        return Ok(None);
    }
    capture_document_snapshot(canonical).map(Some)
}

fn refresh_session_from_loaded(
    existing: &DocumentSession,
    canonical: &Path,
    loaded: LoadedDocumentSource,
    chunk_preset: ChunkPreset,
    rewrite_headings: bool,
    snapshot: Option<DocumentSnapshot>,
) -> RefreshedSession {
    let LoadedDocumentSource {
        source_text: _loaded_source_text,
        regions: loaded_regions,
        region_segmentation_strategy,
        write_back_supported,
        write_back_block_reason,
        plain_text_editor_safe,
        plain_text_editor_block_reason,
    } = loaded;

    let mut session = existing.clone();
    let mut changed = false;
    let canonical_path = canonical.to_string_lossy().to_string();
    if session.document_path != canonical_path {
        session.document_path = canonical_path;
        changed = true;
    }
    if let Some(snapshot) = snapshot {
        session.source_snapshot = Some(snapshot);
        changed = true;
    }

    if should_rebuild_chunks(&session, chunk_preset, rewrite_headings) {
        session.normalized_text = rewrite::normalize_text(&session.source_text);
        session.chunks = build_chunks(
            canonical,
            loaded_regions,
            region_segmentation_strategy,
            chunk_preset,
        );
        session.chunk_preset = Some(chunk_preset);
        session.rewrite_headings = Some(rewrite_headings);
        session.status = RunningState::Idle;
        changed = true;
    }

    if capabilities_changed(
        &session,
        write_back_supported,
        write_back_block_reason.as_deref(),
        plain_text_editor_safe,
        plain_text_editor_block_reason.as_deref(),
    ) {
        session.write_back_supported = write_back_supported;
        session.write_back_block_reason = write_back_block_reason;
        session.plain_text_editor_safe = plain_text_editor_safe;
        session.plain_text_editor_block_reason = plain_text_editor_block_reason;
        changed = true;
    }

    if changed {
        session.updated_at = Utc::now();
    }

    RefreshedSession { session, changed }
}

fn should_rebuild_chunks(
    session: &DocumentSession,
    chunk_preset: ChunkPreset,
    rewrite_headings: bool,
) -> bool {
    if !session.suggestions.is_empty() {
        return false;
    }

    let settings_changed = session.chunk_preset != Some(chunk_preset)
        || session.rewrite_headings != Some(rewrite_headings);
    if settings_changed {
        return true;
    }

    let rebuilt = session
        .chunks
        .iter()
        .map(|chunk| format!("{}{}", chunk.source_text, chunk.separator_after))
        .collect::<String>();
    let format = document_format(Path::new(&session.document_path));
    let has_inline_newlines = session.chunks.iter().any(chunk_has_inline_newlines);
    let allow_inline_newlines = format == crate::models::DocumentFormat::Tex;

    rebuilt != session.source_text
        || (!matches!(chunk_preset, ChunkPreset::Paragraph)
            && has_inline_newlines
            && !allow_inline_newlines)
}

fn chunk_has_inline_newlines(chunk: &ChunkTask) -> bool {
    !chunk.skip_rewrite && (chunk.source_text.contains('\n') || chunk.source_text.contains('\r'))
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

fn capabilities_changed(
    session: &DocumentSession,
    write_back_supported: bool,
    write_back_block_reason: Option<&str>,
    plain_text_editor_safe: bool,
    plain_text_editor_block_reason: Option<&str>,
) -> bool {
    session.write_back_supported != write_back_supported
        || session.write_back_block_reason.as_deref() != write_back_block_reason
        || session.plain_text_editor_safe != plain_text_editor_safe
        || session.plain_text_editor_block_reason.as_deref() != plain_text_editor_block_reason
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use chrono::Utc;

    use super::refresh_session_from_loaded;
    use crate::{
        adapters::TextRegion,
        documents::{LoadedDocumentSource, RegionSegmentationStrategy},
        models::{
            ChunkPreset, ChunkStatus, ChunkTask, DocumentSession, DocumentSnapshot, RunningState,
        },
    };

    fn sample_session() -> DocumentSession {
        let now = Utc::now();
        DocumentSession {
            id: "session-1".to_string(),
            title: "示例".to_string(),
            document_path: "/tmp/example.docx".to_string(),
            source_text: "前文E=mc^2后文".to_string(),
            source_snapshot: None,
            normalized_text: "前文E=mc^2后文".to_string(),
            write_back_supported: true,
            write_back_block_reason: None,
            plain_text_editor_safe: false,
            plain_text_editor_block_reason: Some(
                "当前文档包含行内锁定内容（如公式、分页符或占位符），暂不支持在纯文本编辑器中直接写回。"
                    .to_string(),
            ),
            chunk_preset: Some(ChunkPreset::Paragraph),
            rewrite_headings: Some(false),
            chunks: vec![ChunkTask {
                index: 0,
                source_text: "前文E=mc^2后文".to_string(),
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

    fn loaded_docx() -> LoadedDocumentSource {
        LoadedDocumentSource {
            source_text: "前文E=mc^2后文".to_string(),
            regions: vec![
                TextRegion {
                    body: "前文".to_string(),
                    skip_rewrite: false,
                    presentation: None,
                },
                TextRegion {
                    body: "E=mc^2".to_string(),
                    skip_rewrite: true,
                    presentation: None,
                },
                TextRegion {
                    body: "后文".to_string(),
                    skip_rewrite: false,
                    presentation: None,
                },
            ],
            region_segmentation_strategy: RegionSegmentationStrategy::PreserveBoundaries,
            write_back_supported: true,
            write_back_block_reason: None,
            plain_text_editor_safe: true,
            plain_text_editor_block_reason: None,
        }
    }

    #[test]
    fn refreshes_stale_plain_text_editor_capability() {
        let existing = sample_session();
        let refreshed = refresh_session_from_loaded(
            &existing,
            Path::new("/tmp/example.docx"),
            loaded_docx(),
            crate::models::ChunkPreset::Paragraph,
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
    fn rebuilds_clean_session_when_chunk_preset_metadata_is_missing() {
        let now = Utc::now();
        let existing = DocumentSession {
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
            chunk_preset: None,
            rewrite_headings: None,
            chunks: vec![
                ChunkTask {
                    index: 0,
                    source_text: "第一句。".to_string(),
                    separator_after: String::new(),
                    skip_rewrite: false,
                    presentation: None,
                    status: ChunkStatus::Idle,
                    error_message: None,
                },
                ChunkTask {
                    index: 1,
                    source_text: "第二句。".to_string(),
                    separator_after: String::new(),
                    skip_rewrite: false,
                    presentation: None,
                    status: ChunkStatus::Idle,
                    error_message: None,
                },
            ],
            suggestions: Vec::new(),
            next_suggestion_sequence: 1,
            status: RunningState::Idle,
            created_at: now,
            updated_at: now,
        };
        let loaded = LoadedDocumentSource {
            source_text: "第一句。第二句。".to_string(),
            regions: vec![TextRegion {
                body: "第一句。第二句。".to_string(),
                skip_rewrite: false,
                presentation: None,
            }],
            region_segmentation_strategy: RegionSegmentationStrategy::PreserveBoundaries,
            write_back_supported: true,
            write_back_block_reason: None,
            plain_text_editor_safe: true,
            plain_text_editor_block_reason: None,
        };

        let refreshed = refresh_session_from_loaded(
            &existing,
            Path::new("/tmp/example.docx"),
            loaded,
            ChunkPreset::Paragraph,
            false,
            None,
        );

        assert!(refreshed.changed);
        assert_eq!(refreshed.session.chunk_preset, Some(ChunkPreset::Paragraph));
        assert_eq!(refreshed.session.rewrite_headings, Some(false));
        assert_eq!(refreshed.session.chunks.len(), 1);
        assert_eq!(refreshed.session.chunks[0].source_text, "第一句。第二句。");
    }
}
