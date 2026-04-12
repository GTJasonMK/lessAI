use std::{fs, path::Path};

use uuid::Uuid;

use crate::{
    adapters, atomic_write::write_bytes_atomically,
    document_snapshot::ensure_document_snapshot_matches, models,
};

const PDF_WRITE_BACK_UNSUPPORTED: &str = "当前文件为 .pdf：暂不支持写回覆盖（PDF 不是纯文本格式）。请使用“导出”为 .txt 后再进行后续排版。";
const WRITEBACK_SOURCE_MISMATCH_ERROR: &str =
    "原文件内容与当前会话不一致，文件可能已在外部发生变化。为避免误写，请重新导入。";
const DOCX_WRITEBACK_SOURCE_MISMATCH_ERROR: &str =
    "docx 原文件内容与当前会话不一致，文件可能已在外部发生变化。为避免误写，请重新导入。";

pub(crate) fn document_session_id(document_path: &str) -> String {
    // 用 UUID v5 将“文档路径”稳定映射为 session id：
    // - 同一台机器上同一路径 => 同一个 id（用于恢复进度）
    // - 避免把路径直接当文件名（包含非法字符/过长）
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
        // PDF 导入后会先抽取成“纯文本”，因此后续流程按 PlainText 处理即可。
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
    pub(crate) regions: Vec<adapters::TextRegion>,
    pub(crate) region_segmentation_strategy: RegionSegmentationStrategy,
    pub(crate) write_back_supported: bool,
    pub(crate) write_back_block_reason: Option<String>,
    pub(crate) plain_text_editor_safe: bool,
    pub(crate) plain_text_editor_block_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RegionSegmentationStrategy {
    FormatAware,
    PreserveBoundaries,
}

fn plain_text_regions(text: &str) -> Vec<adapters::TextRegion> {
    vec![adapters::TextRegion {
        body: text.to_string(),
        skip_rewrite: false,
        presentation: None,
    }]
}

pub(crate) fn detect_document_capabilities(
    path: &Path,
) -> Result<(bool, Option<String>, bool, Option<String>), String> {
    if is_pdf_path(path) {
        return Ok((
            false,
            Some(PDF_WRITE_BACK_UNSUPPORTED.to_string()),
            false,
            Some(PDF_WRITE_BACK_UNSUPPORTED.to_string()),
        ));
    }
    if !is_docx_path(path) {
        return Ok((true, None, true, None));
    }

    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let write_back_block_reason = adapters::docx::DocxAdapter::validate_writeback(&bytes).err();
    let plain_text_editor_block_reason = write_back_block_reason.clone();
    Ok((
        write_back_block_reason.is_none(),
        write_back_block_reason,
        plain_text_editor_block_reason.is_none(),
        plain_text_editor_block_reason,
    ))
}

fn decode_utf16_payload(payload: &[u8], little_endian: bool) -> Result<String, String> {
    if payload.len() % 2 != 0 {
        return Err("文本编码疑似为 UTF-16，但字节长度不是 2 的倍数，无法解码。".to_string());
    }

    let mut words: Vec<u16> = Vec::with_capacity(payload.len() / 2);
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

fn decode_text_file(bytes: &[u8]) -> Result<String, String> {
    if bytes.is_empty() {
        return Ok(String::new());
    }

    // UTF-8 BOM
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return std::str::from_utf8(&bytes[3..])
            .map(|value| value.to_string())
            .map_err(|error| format!("UTF-8 解码失败：{error}"));
    }

    // UTF-16 LE BOM
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return decode_utf16_payload(&bytes[2..], true);
    }

    // UTF-16 BE BOM
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return decode_utf16_payload(&bytes[2..], false);
    }

    // 默认按 UTF-8 读取（推荐）
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
    let ext = path_extension_lower(path);

    match ext.as_deref() {
        Some("docx") => {
            let bytes = fs::read(path).map_err(|error| error.to_string())?;
            let regions = adapters::docx::DocxAdapter::extract_regions(&bytes, rewrite_headings)?;
            let source_text = regions
                .iter()
                .map(|region| region.body.as_str())
                .collect::<String>();
            if source_text.trim().is_empty() {
                return Err(
                    "未从 docx 中抽取到可见文本。该文件可能只有图片/公式/表格，或正文不在 document.xml 中。"
                        .to_string(),
                );
            }
            let write_back_block_reason =
                adapters::docx::DocxAdapter::validate_writeback(&bytes).err();
            let plain_text_editor_block_reason = write_back_block_reason.clone();
            Ok(LoadedDocumentSource {
                source_text,
                regions,
                region_segmentation_strategy: RegionSegmentationStrategy::PreserveBoundaries,
                write_back_supported: write_back_block_reason.is_none(),
                write_back_block_reason,
                plain_text_editor_safe: plain_text_editor_block_reason.is_none(),
                plain_text_editor_block_reason,
            })
        }
        Some("doc") => {
            Err("暂不支持 .doc（老版 Word 二进制格式）。请另存为 .docx 后再导入。".to_string())
        }
        Some("pdf") => {
            let bytes = fs::read(path).map_err(|error| error.to_string())?;
            let source_text = adapters::pdf::PdfAdapter::extract_text(&bytes)?;
            Ok(LoadedDocumentSource {
                regions: plain_text_regions(&source_text),
                source_text,
                region_segmentation_strategy: RegionSegmentationStrategy::FormatAware,
                write_back_supported: false,
                write_back_block_reason: Some(PDF_WRITE_BACK_UNSUPPORTED.to_string()),
                plain_text_editor_safe: false,
                plain_text_editor_block_reason: Some(PDF_WRITE_BACK_UNSUPPORTED.to_string()),
            })
        }
        Some(other) => {
            if !matches!(other, "txt" | "md" | "markdown" | "tex" | "latex") {
                return Err(format!(
                    "暂不支持该文件格式：.{other}。当前支持：{}。",
                    supported_extensions_hint()
                ));
            }
            let bytes = fs::read(path).map_err(|error| error.to_string())?;
            let source_text = decode_text_file(&bytes)?;
            let regions = match other {
                "md" | "markdown" => adapters::markdown::MarkdownAdapter::split_regions(
                    &source_text,
                    rewrite_headings,
                ),
                "tex" | "latex" => {
                    adapters::tex::TexAdapter::split_regions(&source_text, rewrite_headings)
                }
                _ => plain_text_regions(&source_text),
            };
            Ok(LoadedDocumentSource {
                source_text,
                regions,
                region_segmentation_strategy: RegionSegmentationStrategy::FormatAware,
                write_back_supported: true,
                write_back_block_reason: None,
                plain_text_editor_safe: true,
                plain_text_editor_block_reason: None,
            })
        }
        None => {
            let bytes = fs::read(path).map_err(|error| error.to_string())?;
            let source_text = decode_text_file(&bytes)?;
            let regions = plain_text_regions(&source_text);
            Ok(LoadedDocumentSource {
                source_text,
                regions,
                region_segmentation_strategy: RegionSegmentationStrategy::FormatAware,
                write_back_supported: true,
                write_back_block_reason: None,
                plain_text_editor_safe: true,
                plain_text_editor_block_reason: None,
            })
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
    expected_source_text: &str,
    expected_source_snapshot: Option<&models::DocumentSnapshot>,
) -> Result<(), String> {
    if is_pdf_path(path) {
        return Ok(());
    }
    load_verified_writeback_bytes(path, expected_source_text, expected_source_snapshot).map(|_| ())
}

pub(crate) fn ensure_document_can_ai_rewrite_safely(
    path: &Path,
    expected_source_text: &str,
    expected_source_snapshot: Option<&models::DocumentSnapshot>,
    write_back_supported: bool,
    write_back_block_reason: Option<&str>,
) -> Result<(), String> {
    ensure_document_can_ai_rewrite(path, write_back_supported, write_back_block_reason)?;
    ensure_document_source_matches_session(path, expected_source_text, expected_source_snapshot)
}

pub(crate) fn write_document_content(
    path: &Path,
    expected_source_text: &str,
    expected_source_snapshot: Option<&models::DocumentSnapshot>,
    updated_text: &str,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let current_bytes =
        load_verified_writeback_bytes(path, expected_source_text, expected_source_snapshot)?;

    if is_docx_path(path) {
        let updated = adapters::docx::DocxAdapter::write_updated_text(
            &current_bytes,
            expected_source_text,
            updated_text,
        )?;
        write_bytes_atomically(path, &updated)?;
        return Ok(());
    }

    write_bytes_atomically(path, updated_text.as_bytes())
}

pub(crate) fn write_document_regions(
    path: &Path,
    expected_source_text: &str,
    expected_source_snapshot: Option<&models::DocumentSnapshot>,
    updated_regions: &[adapters::TextRegion],
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let current_bytes =
        load_verified_writeback_bytes(path, expected_source_text, expected_source_snapshot)?;
    if !is_docx_path(path) {
        return Err("当前仅 docx 支持按片段写回。".to_string());
    }

    let updated = adapters::docx::DocxAdapter::write_updated_regions(
        &current_bytes,
        expected_source_text,
        updated_regions,
    )?;
    write_bytes_atomically(path, &updated)
}

pub(crate) fn load_verified_writeback_bytes(
    path: &Path,
    expected_source_text: &str,
    expected_source_snapshot: Option<&models::DocumentSnapshot>,
) -> Result<Vec<u8>, String> {
    if expected_source_snapshot.is_some() {
        return ensure_document_snapshot_matches(path, expected_source_snapshot);
    }

    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    verify_writeback_source_matches(path, &bytes, expected_source_text)?;
    Ok(bytes)
}

fn verify_writeback_source_matches(
    path: &Path,
    bytes: &[u8],
    expected_source_text: &str,
) -> Result<(), String> {
    if is_docx_path(path) {
        let current = adapters::docx::DocxAdapter::extract_writeback_source_text(bytes)?;
        if current != expected_source_text {
            return Err(DOCX_WRITEBACK_SOURCE_MISMATCH_ERROR.to_string());
        }
        return Ok(());
    }

    let current = decode_text_file(bytes)?;
    if current != expected_source_text {
        return Err(WRITEBACK_SOURCE_MISMATCH_ERROR.to_string());
    }
    Ok(())
}

#[cfg(test)]
#[path = "documents_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "documents_writeback_tests.rs"]
mod writeback_tests;
