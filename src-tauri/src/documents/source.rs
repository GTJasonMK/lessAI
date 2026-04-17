use std::{fs, path::Path};

use uuid::Uuid;

use crate::{
    adapters, models,
    rewrite_unit::{WritebackSlot, WritebackSlotRole},
};

pub(super) const PDF_WRITE_BACK_UNSUPPORTED: &str =
    "当前文件为 .pdf：暂不支持写回覆盖（PDF 不是纯文本格式）。请使用“导出”为 .txt 后再进行后续排版。";
const PRESERVED_BLOCK_SEPARATOR: &str = "\n\n";

pub(crate) fn document_session_id(document_path: &str) -> String {
    let namespace = Uuid::from_bytes([
        0x6c, 0x65, 0x73, 0x73, 0x61, 0x69, 0x2d, 0x64, 0x6f, 0x63, 0x2d, 0x6e, 0x73, 0x2d, 0x30,
        0x31,
    ]);
    Uuid::new_v5(&namespace, document_path.as_bytes()).to_string()
}

fn path_extension_lower(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
}

fn supported_extensions() -> &'static [&'static str] {
    &["txt", "md", "markdown", "tex", "latex", "docx", "pdf"]
}

fn supported_extensions_hint() -> String {
    supported_extensions()
        .iter()
        .map(|ext| format!(".{ext}"))
        .collect::<Vec<_>>()
        .join(" / ")
}

pub(crate) fn document_format(path: &Path) -> models::DocumentFormat {
    match path_extension_lower(path).as_deref() {
        Some("md") | Some("markdown") => models::DocumentFormat::Markdown,
        Some("tex") | Some("latex") => models::DocumentFormat::Tex,
        Some("pdf") => models::DocumentFormat::PlainText,
        _ => models::DocumentFormat::PlainText,
    }
}

pub(crate) fn is_docx_path(path: &Path) -> bool {
    path_extension_lower(path).as_deref() == Some("docx")
}

pub(crate) fn is_pdf_path(path: &Path) -> bool {
    path_extension_lower(path).as_deref() == Some("pdf")
}

pub(crate) struct LoadedDocumentSource {
    pub(crate) source_text: String,
    pub(crate) writeback_slots: Vec<WritebackSlot>,
    pub(crate) write_back_supported: bool,
    pub(crate) write_back_block_reason: Option<String>,
    pub(crate) plain_text_editor_safe: bool,
    pub(crate) plain_text_editor_block_reason: Option<String>,
}

fn plain_text_regions(text: &str) -> Vec<adapters::TextRegion> {
    vec![adapters::TextRegion {
        body: text.to_string(),
        skip_rewrite: false,
        presentation: None,
    }]
}

pub(crate) fn writeback_slots_from_regions(regions: &[adapters::TextRegion]) -> Vec<WritebackSlot> {
    regions
        .iter()
        .enumerate()
        .map(|(index, region)| build_writeback_slot(index, region))
        .collect()
}

fn build_writeback_slot(index: usize, region: &adapters::TextRegion) -> WritebackSlot {
    let (text, separator_after) = split_region_body_and_separator(&region.body);
    let text_empty = text.is_empty();
    let whitespace_only = !text.is_empty() && text.chars().all(|ch| ch.is_whitespace());
    let editable = !region.skip_rewrite && !whitespace_only && !text_empty;

    WritebackSlot {
        id: format!("slot-{index}"),
        order: index,
        text,
        editable,
        role: slot_role(text_empty, region.skip_rewrite, whitespace_only, &separator_after),
        presentation: region.presentation.clone(),
        anchor: None,
        separator_after,
    }
}

fn slot_role(
    text_empty: bool,
    skip_rewrite: bool,
    whitespace_only: bool,
    separator_after: &str,
) -> WritebackSlotRole {
    if text_empty && separator_after.contains(PRESERVED_BLOCK_SEPARATOR) {
        return WritebackSlotRole::ParagraphBreak;
    }
    if skip_rewrite || whitespace_only {
        return WritebackSlotRole::LockedText;
    }
    WritebackSlotRole::EditableText
}

fn split_region_body_and_separator(body: &str) -> (String, String) {
    if let Some(text) = body.strip_suffix(PRESERVED_BLOCK_SEPARATOR) {
        return (text.to_string(), PRESERVED_BLOCK_SEPARATOR.to_string());
    }
    split_trailing_whitespace(body)
}

fn split_trailing_whitespace(text: &str) -> (String, String) {
    let split_at = text
        .char_indices()
        .rev()
        .find_map(|(index, ch)| (!ch.is_whitespace()).then_some(index + ch.len_utf8()))
        .unwrap_or(0);
    (text[..split_at].to_string(), text[split_at..].to_string())
}

fn decode_utf16_payload(payload: &[u8], little_endian: bool) -> Result<String, String> {
    if payload.len() % 2 != 0 {
        return Err("文本编码疑似为 UTF-16，但字节长度不是 2 的倍数，无法解码。".to_string());
    }

    let mut words = Vec::with_capacity(payload.len() / 2);
    for chunk in payload.chunks_exact(2) {
        let word = if little_endian {
            u16::from_le_bytes([chunk[0], chunk[1]])
        } else {
            u16::from_be_bytes([chunk[0], chunk[1]])
        };
        words.push(word);
    }

    String::from_utf16(&words).map_err(|error| format!("UTF-16 解码失败：{error}"))
}

pub(crate) fn decode_text_file(bytes: &[u8]) -> Result<String, String> {
    if bytes.is_empty() {
        return Ok(String::new());
    }
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return std::str::from_utf8(&bytes[3..])
            .map(|value| value.to_string())
            .map_err(|error| format!("UTF-8 解码失败：{error}"));
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return decode_utf16_payload(&bytes[2..], true);
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return decode_utf16_payload(&bytes[2..], false);
    }

    std::str::from_utf8(bytes)
        .map(|value| value.to_string())
        .map_err(|_| {
            "无法读取文本文件：当前仅支持 UTF-8（推荐）或 UTF-16（需带 BOM）。该文件可能是 GBK/ANSI 编码或二进制格式，请先转换为 UTF-8 后再导入。".to_string()
        })
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
        Some(other) => load_textual_source(path, other, rewrite_headings),
        None => load_plain_text_source(path),
    }
}

fn load_docx_source(path: &Path, rewrite_headings: bool) -> Result<LoadedDocumentSource, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let writeback_slots =
        adapters::docx::DocxAdapter::extract_writeback_slots(&bytes, rewrite_headings)?;
    let source_text = writeback_slots
        .iter()
        .map(|slot| format!("{}{}", slot.text, slot.separator_after))
        .collect::<String>();
    if source_text.trim().is_empty() {
        return Err(
            "未从 docx 中抽取到可见文本。该文件可能只有图片/公式/表格，或正文不在 document.xml 中。"
                .to_string(),
        );
    }
    let write_back_block_reason = adapters::docx::DocxAdapter::validate_writeback(&bytes).err();
    let plain_text_editor_block_reason = write_back_block_reason.clone();
    Ok(LoadedDocumentSource {
        source_text,
        writeback_slots,
        write_back_supported: write_back_block_reason.is_none(),
        write_back_block_reason,
        plain_text_editor_safe: plain_text_editor_block_reason.is_none(),
        plain_text_editor_block_reason,
    })
}

fn load_pdf_source(path: &Path) -> Result<LoadedDocumentSource, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let source_text = adapters::pdf::PdfAdapter::extract_text(&bytes)?;
    let writeback_slots = writeback_slots_from_regions(&plain_text_regions(&source_text));
    Ok(LoadedDocumentSource {
        source_text,
        writeback_slots,
        write_back_supported: false,
        write_back_block_reason: Some(PDF_WRITE_BACK_UNSUPPORTED.to_string()),
        plain_text_editor_safe: false,
        plain_text_editor_block_reason: Some(PDF_WRITE_BACK_UNSUPPORTED.to_string()),
    })
}

fn load_textual_source(
    path: &Path,
    extension: &str,
    rewrite_headings: bool,
) -> Result<LoadedDocumentSource, String> {
    if !matches!(extension, "txt" | "md" | "markdown" | "tex" | "latex") {
        return Err(format!(
            "暂不支持该文件格式：.{extension}。当前支持：{}。",
            supported_extensions_hint()
        ));
    }

    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let source_text = decode_text_file(&bytes)?;
    let regions = match extension {
        "md" | "markdown" => {
            adapters::markdown::MarkdownAdapter::split_regions(&source_text, rewrite_headings)
        }
        "tex" | "latex" => adapters::tex::TexAdapter::split_regions(&source_text, rewrite_headings),
        _ => plain_text_regions(&source_text),
    };
    let writeback_slots = writeback_slots_from_regions(&regions);
    Ok(LoadedDocumentSource {
        source_text,
        writeback_slots,
        write_back_supported: true,
        write_back_block_reason: None,
        plain_text_editor_safe: true,
        plain_text_editor_block_reason: None,
    })
}

fn load_plain_text_source(path: &Path) -> Result<LoadedDocumentSource, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let source_text = decode_text_file(&bytes)?;
    let writeback_slots = writeback_slots_from_regions(&plain_text_regions(&source_text));
    Ok(LoadedDocumentSource {
        source_text,
        writeback_slots,
        write_back_supported: true,
        write_back_block_reason: None,
        plain_text_editor_safe: true,
        plain_text_editor_block_reason: None,
    })
}
