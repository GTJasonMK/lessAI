use std::path::Path;

use serde::Deserialize;

use crate::{
    adapters, atomic_write::write_bytes_atomically,
    document_snapshot::ensure_document_snapshot_matches, models, rewrite,
    rewrite_unit::WritebackSlot, textual_template,
};
use crate::session_capability_models::{CapabilityGate, DocumentBackendKind};

use super::{
    capabilities::{document_backend_kind, ensure_capability_allowed},
    source::PDF_WRITE_BACK_UNSUPPORTED,
    textual::load_textual_template_source,
};

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

#[derive(Clone, Copy, Debug)]
pub(crate) struct DocumentWritebackContext<'a> {
    pub expected_source_text: &'a str,
    pub expected_source_snapshot: Option<&'a models::DocumentSnapshot>,
    pub expected_template_signature: Option<&'a str>,
    pub expected_slot_structure_signature: Option<&'a str>,
    pub rewrite_headings: bool,
}

impl<'a> DocumentWritebackContext<'a> {
    pub(crate) fn new(
        expected_source_text: &'a str,
        expected_source_snapshot: Option<&'a models::DocumentSnapshot>,
    ) -> Self {
        Self {
            expected_source_text,
            expected_source_snapshot,
            expected_template_signature: None,
            expected_slot_structure_signature: None,
            rewrite_headings: false,
        }
    }

    pub(crate) fn from_session(session: &'a models::DocumentSession) -> Self {
        Self::new(&session.source_text, session.source_snapshot.as_ref()).with_structure_signatures(
            session.template_signature.as_deref(),
            session.slot_structure_signature.as_deref(),
            session.rewrite_headings.unwrap_or(false),
        )
    }

    pub(crate) fn with_structure_signatures(
        mut self,
        expected_template_signature: Option<&'a str>,
        expected_slot_structure_signature: Option<&'a str>,
        rewrite_headings: bool,
    ) -> Self {
        self.expected_template_signature = expected_template_signature;
        self.expected_slot_structure_signature = expected_slot_structure_signature;
        self.rewrite_headings = rewrite_headings;
        self
    }
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

#[cfg(test)]
pub(crate) fn ensure_document_can_write_back(path: &str) -> Result<(), String> {
    if document_backend_kind(Path::new(path)) == DocumentBackendKind::Pdf {
        return Err(PDF_WRITE_BACK_UNSUPPORTED.to_string());
    }
    Ok(())
}

pub(crate) fn ensure_document_can_ai_rewrite(ai_rewrite: &CapabilityGate) -> Result<(), String> {
    ensure_capability_allowed(
        ai_rewrite,
        "当前文档暂不支持安全写回覆盖，因此不允许继续 AI 改写。",
    )
}

pub(crate) fn ensure_document_source_matches_session(
    path: &Path,
    expected_source_snapshot: Option<&models::DocumentSnapshot>,
) -> Result<(), String> {
    if document_backend_kind(path) == DocumentBackendKind::Pdf {
        return Ok(());
    }
    load_verified_writeback_source(path, expected_source_snapshot, false).map(|_| ())
}

pub(crate) fn ensure_document_can_ai_rewrite_safely(
    path: &Path,
    expected_source_snapshot: Option<&models::DocumentSnapshot>,
    ai_rewrite: &CapabilityGate,
) -> Result<(), String> {
    ensure_document_can_ai_rewrite(ai_rewrite)?;
    ensure_document_source_matches_session(path, expected_source_snapshot)
}

pub(crate) fn execute_document_writeback(
    path: &Path,
    context: DocumentWritebackContext<'_>,
    writeback: DocumentWriteback<'_>,
    mode: WritebackMode,
) -> Result<(), String> {
    if document_backend_kind(path) == DocumentBackendKind::Pdf {
        return match (mode, writeback) {
            (WritebackMode::Validate, DocumentWriteback::Text(_)) => Ok(()),
            _ => Err(PDF_WRITE_BACK_UNSUPPORTED.to_string()),
        };
    }

    let source = load_verified_writeback_source(
        path,
        context.expected_source_snapshot,
        context.rewrite_headings,
    )?;
    let updated = match writeback {
        DocumentWriteback::Text(updated_text) => {
            build_text_writeback_bytes(&source, context.expected_source_text, updated_text)
        }
        DocumentWriteback::Slots(updated_slots) => {
            build_slot_writeback_bytes(&source, context, updated_slots)
        }
    }?;
    finish_document_writeback(path, &updated, mode)
}

enum VerifiedWritebackSource {
    Textual(textual_template::TextTemplate),
    Docx {
        bytes: Vec<u8>,
        source: adapters::docx::LoadedDocxWritebackSource,
        model: adapters::docx::DocxWritebackModel,
    },
}

fn load_verified_writeback_source(
    path: &Path,
    expected_source_snapshot: Option<&models::DocumentSnapshot>,
    rewrite_headings: bool,
) -> Result<VerifiedWritebackSource, String> {
    let source_bytes = ensure_document_snapshot_matches(path, expected_source_snapshot)?;
    match document_backend_kind(path) {
        DocumentBackendKind::Textual => {
            let (_, template) =
                load_textual_template_source(path, &source_bytes, rewrite_headings)?;
            Ok(VerifiedWritebackSource::Textual(template))
        }
        DocumentBackendKind::Docx => {
            let source = adapters::docx::DocxAdapter::load_writeback_source(&source_bytes)?;
            let model =
                adapters::docx::DocxAdapter::extract_writeback_model_from_source(&source, rewrite_headings);
            Ok(VerifiedWritebackSource::Docx {
                bytes: source_bytes,
                source,
                model,
            })
        }
        DocumentBackendKind::Pdf => Err(PDF_WRITE_BACK_UNSUPPORTED.to_string()),
    }
}

fn build_text_writeback_bytes(
    source: &VerifiedWritebackSource,
    expected_source_text: &str,
    updated_text: &str,
) -> Result<Vec<u8>, String> {
    match source {
        VerifiedWritebackSource::Textual(_) => Ok(normalize_text_against_source_layout(
            expected_source_text,
            updated_text,
        )
        .into_bytes()),
        VerifiedWritebackSource::Docx {
            bytes: current_bytes,
            source,
            ..
        } => adapters::docx::DocxAdapter::write_updated_text_with_source(
                current_bytes,
                source,
                expected_source_text,
                updated_text,
            ),
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
    context: DocumentWritebackContext<'_>,
    updated_slots: &[WritebackSlot],
) -> Result<Vec<u8>, String> {
    match source {
        VerifiedWritebackSource::Textual(template) => {
            textual_template::validate::ensure_template_signature(
                context.expected_template_signature,
                template,
            )?;
            textual_template::validate::ensure_slot_structure_signature(
                context.expected_slot_structure_signature,
                updated_slots,
            )?;
            let rebuilt = textual_template::rebuild::rebuild_text(template, updated_slots)?;
            Ok(
                normalize_text_against_source_layout(context.expected_source_text, &rebuilt)
                    .into_bytes(),
            )
        }
        VerifiedWritebackSource::Docx {
            bytes: current_bytes,
            source,
            model,
        } => {
            textual_template::validate::ensure_signature_matches(
                context.expected_template_signature,
                &model.template_signature,
                "当前会话缺少 docx 模板签名，无法校验结构一致性。",
                "当前 docx 模板结构与会话记录不一致，无法安全继续。",
            )?;
            textual_template::validate::ensure_signature_matches(
                context.expected_slot_structure_signature,
                &model.slot_structure_signature,
                "当前会话缺少 docx 槽位结构签名，无法校验写回边界。",
                "当前 docx 槽位结构与会话记录不一致，无法安全继续。",
            )?;
            textual_template::validate::ensure_slot_structure_signature(
                context.expected_slot_structure_signature,
                updated_slots,
            )?;
            adapters::docx::DocxAdapter::write_updated_slots_with_source(
                current_bytes,
                source,
                context.expected_source_text,
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
