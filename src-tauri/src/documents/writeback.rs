use std::path::Path;

use serde::Deserialize;

use crate::{
    adapters, atomic_write::write_bytes_atomically,
    document_snapshot::ensure_document_snapshot_matches, models, rewrite,
    rewrite_unit::WritebackSlot,
};

use super::source::{is_docx_path, is_pdf_path, PDF_WRITE_BACK_UNSUPPORTED};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum WritebackMode {
    Validate,
    Write,
}

#[derive(Debug)]
pub(crate) enum DocumentWriteback<'a> {
    Text(&'a str),
    Slots(&'a [WritebackSlot]),
}

#[derive(Debug)]
pub(crate) enum OwnedDocumentWriteback {
    Text(String),
    Slots(Vec<WritebackSlot>),
}

impl OwnedDocumentWriteback {
    pub(crate) fn as_document_writeback(&self) -> DocumentWriteback<'_> {
        match self {
            OwnedDocumentWriteback::Text(updated_text) => DocumentWriteback::Text(updated_text),
            OwnedDocumentWriteback::Slots(updated_slots) => DocumentWriteback::Slots(updated_slots),
        }
    }
}

pub(crate) fn ensure_document_can_write_back(path: &str) -> Result<(), String> {
    if is_pdf_path(Path::new(path)) {
        return Err(PDF_WRITE_BACK_UNSUPPORTED.to_string());
    }
    Ok(())
}

pub(crate) fn ensure_document_can_ai_rewrite(
    path: &Path,
    write_back_supported: bool,
    write_back_block_reason: Option<&str>,
) -> Result<(), String> {
    if is_pdf_path(path) {
        return Ok(());
    }
    if write_back_supported {
        return Ok(());
    }
    Err(write_back_block_reason
        .unwrap_or("当前文档暂不支持安全写回覆盖，因此不允许继续 AI 改写。")
        .to_string())
}

pub(crate) fn ensure_document_source_matches_session(
    path: &Path,
    expected_source_snapshot: Option<&models::DocumentSnapshot>,
) -> Result<(), String> {
    if is_pdf_path(path) {
        return Ok(());
    }
    load_verified_writeback_source(path, expected_source_snapshot).map(|_| ())
}

pub(crate) fn ensure_document_can_ai_rewrite_safely(
    path: &Path,
    expected_source_snapshot: Option<&models::DocumentSnapshot>,
    write_back_supported: bool,
    write_back_block_reason: Option<&str>,
) -> Result<(), String> {
    ensure_document_can_ai_rewrite(path, write_back_supported, write_back_block_reason)?;
    ensure_document_source_matches_session(path, expected_source_snapshot)
}

pub(crate) fn execute_document_writeback(
    path: &Path,
    expected_source_text: &str,
    expected_source_snapshot: Option<&models::DocumentSnapshot>,
    writeback: DocumentWriteback<'_>,
    mode: WritebackMode,
) -> Result<(), String> {
    let source = load_verified_writeback_source(path, expected_source_snapshot)?;
    let updated = build_document_writeback_bytes(&source, expected_source_text, writeback)?;
    finish_document_writeback(path, &updated, mode)
}

fn build_document_writeback_bytes(
    source: &VerifiedWritebackSource,
    expected_source_text: &str,
    writeback: DocumentWriteback<'_>,
) -> Result<Vec<u8>, String> {
    match writeback {
        DocumentWriteback::Text(updated_text) => {
            build_text_writeback_bytes(source, expected_source_text, updated_text)
        }
        DocumentWriteback::Slots(updated_slots) => {
            build_slot_writeback_bytes(source, expected_source_text, updated_slots)
        }
    }
}

enum VerifiedWritebackSource {
    PlainText,
    Docx(Vec<u8>),
}

fn load_verified_writeback_source(
    path: &Path,
    expected_source_snapshot: Option<&models::DocumentSnapshot>,
) -> Result<VerifiedWritebackSource, String> {
    let source_bytes = ensure_document_snapshot_matches(path, expected_source_snapshot)?;
    if !is_docx_path(path) {
        return Ok(VerifiedWritebackSource::PlainText);
    }

    Ok(VerifiedWritebackSource::Docx(source_bytes))
}

fn build_text_writeback_bytes(
    source: &VerifiedWritebackSource,
    expected_source_text: &str,
    updated_text: &str,
) -> Result<Vec<u8>, String> {
    match source {
        VerifiedWritebackSource::PlainText => Ok(normalize_text_against_source_layout(
            expected_source_text,
            updated_text,
        )
        .into_bytes()),
        VerifiedWritebackSource::Docx(current_bytes) => {
            adapters::docx::DocxAdapter::write_updated_text(
                current_bytes,
                expected_source_text,
                updated_text,
            )
        }
    }
}

pub(crate) fn normalize_text_against_source_layout(
    expected_source_text: &str,
    updated_text: &str,
) -> String {
    let line_ending = rewrite::detect_line_ending(expected_source_text);
    let mut normalized = updated_text.to_string();
    if !rewrite::has_trailing_spaces_per_line(expected_source_text) {
        normalized = rewrite::strip_trailing_spaces_per_line(&normalized);
    }
    rewrite::convert_line_endings(&normalized, line_ending)
}

fn build_slot_writeback_bytes(
    source: &VerifiedWritebackSource,
    expected_source_text: &str,
    updated_slots: &[WritebackSlot],
) -> Result<Vec<u8>, String> {
    match source {
        VerifiedWritebackSource::PlainText => Err("当前仅 docx 支持按槽位写回。".to_string()),
        VerifiedWritebackSource::Docx(current_bytes) => {
            adapters::docx::DocxAdapter::write_updated_slots(
                current_bytes,
                expected_source_text,
                updated_slots,
            )
        }
    }
}

fn finish_document_writeback(
    path: &Path,
    updated: &[u8],
    mode: WritebackMode,
) -> Result<(), String> {
    match mode {
        WritebackMode::Validate => Ok(()),
        WritebackMode::Write => write_bytes_atomically(path, updated),
    }
}

#[cfg(test)]
#[path = "writeback_internal_tests.rs"]
mod internal_tests;
