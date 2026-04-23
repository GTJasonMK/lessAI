use crate::{adapters, models, rewrite_unit::WritebackSlot};
use std::{fs, path::Path};
use uuid::Uuid;

use super::textual::{load_textual_template_source, path_extension_lower};
use super::{capability_gate, DocumentCapabilityPolicy};

pub(super) const PDF_WRITE_BACK_UNSUPPORTED: &str =
    "当前文件为 .pdf：暂不支持写回覆盖（PDF 不是纯文本格式）。请使用“导出”为 .txt 后再进行后续排版。";

pub(crate) fn document_session_id(document_path: &str) -> String {
    let namespace = Uuid::from_bytes([
        0x6c, 0x65, 0x73, 0x73, 0x61, 0x69, 0x2d, 0x64, 0x6f, 0x63, 0x2d, 0x6e, 0x73, 0x2d, 0x30,
        0x31,
    ]);
    Uuid::new_v5(&namespace, document_path.as_bytes()).to_string()
}

pub(crate) struct LoadedDocumentSource {
    pub(crate) source_text: String,
    pub(crate) template_kind: Option<String>,
    pub(crate) template_signature: Option<String>,
    pub(crate) slot_structure_signature: Option<String>,
    pub(crate) template_snapshot: Option<crate::textual_template::TextTemplate>,
    pub(crate) writeback_slots: Vec<WritebackSlot>,
    pub(crate) capability_policy: DocumentCapabilityPolicy,
}

pub(crate) fn load_document_source(
    path: &Path,
    rewrite_headings: bool,
) -> Result<LoadedDocumentSource, String> {
    match path_extension_lower(path).as_deref() {
        Some("docx") => load_docx_source(path, rewrite_headings),
        Some("doc") => {
            Err("暂不支持 .doc（老版 Word 二进制格式）。请另存为 .docx 后再导入。".to_string())
        }
        Some("pdf") => load_pdf_source(path),
        _ => load_textual_source(path, rewrite_headings),
    }
}

fn load_docx_source(path: &Path, rewrite_headings: bool) -> Result<LoadedDocumentSource, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let loaded = adapters::docx::DocxAdapter::load_writeback_source(&bytes)?;
    let model =
        adapters::docx::DocxAdapter::extract_writeback_model_from_source(&loaded, rewrite_headings);
    let source_text = model.source_text.clone();
    if source_text.trim().is_empty() {
        return Err(
            "未从 docx 中抽取到可见文本。该文件可能只有图片/公式/表格，或正文不在 document.xml 中。"
                .to_string(),
        );
    }
    Ok(LoadedDocumentSource {
        source_text,
        template_kind: None,
        template_signature: Some(model.template_signature),
        slot_structure_signature: Some(model.slot_structure_signature),
        template_snapshot: None,
        writeback_slots: model.writeback_slots,
        capability_policy: DocumentCapabilityPolicy::new(
            capability_gate(true, None),
            capability_gate(true, None),
        ),
    })
}

fn load_pdf_source(path: &Path) -> Result<LoadedDocumentSource, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let source_text = adapters::pdf::PdfAdapter::extract_text(&bytes)?;
    let writeback_slots = crate::textual_template::factory::build_slots(
        &source_text,
        models::DocumentFormat::PlainText,
        false,
    );
    Ok(LoadedDocumentSource {
        source_text,
        template_kind: None,
        template_signature: None,
        slot_structure_signature: None,
        template_snapshot: None,
        writeback_slots,
        capability_policy: DocumentCapabilityPolicy::new(
            capability_gate(false, Some(PDF_WRITE_BACK_UNSUPPORTED)),
            capability_gate(false, Some(PDF_WRITE_BACK_UNSUPPORTED)),
        ),
    })
}

fn load_textual_source(
    path: &Path,
    rewrite_headings: bool,
) -> Result<LoadedDocumentSource, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let (source_text, template) = load_textual_template_source(path, &bytes, rewrite_headings)?;
    Ok(build_template_loaded_source(source_text, template))
}

fn build_template_loaded_source(
    source_text: String,
    template: crate::textual_template::TextTemplate,
) -> LoadedDocumentSource {
    let template_kind = template.kind.clone();
    let template_signature = template.template_signature.clone();
    let built = crate::textual_template::slots::build_slots(&template);

    LoadedDocumentSource {
        source_text,
        template_kind: Some(template_kind),
        template_signature: Some(template_signature),
        slot_structure_signature: Some(built.slot_structure_signature),
        template_snapshot: Some(template),
        writeback_slots: built.slots,
        capability_policy: DocumentCapabilityPolicy::new(
            capability_gate(true, None),
            capability_gate(true, None),
        ),
    }
}
