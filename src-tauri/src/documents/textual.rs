use std::path::Path;

use crate::{models, textual_template};

const SUPPORTED_EXTENSIONS_HINT: &str = ".txt / .md / .markdown / .tex / .latex / .docx / .pdf";

pub(crate) fn path_extension_lower(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .filter(|value| !value.is_empty())
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

pub(crate) fn load_textual_template_source(
    path: &Path,
    source_bytes: &[u8],
    rewrite_headings: bool,
) -> Result<(String, textual_template::TextTemplate), String> {
    let source_text = decode_text_file(source_bytes)?;
    let format = textual_template_format(path)?;
    let template =
        textual_template::factory::build_template(&source_text, format, rewrite_headings);
    Ok((source_text, template))
}

pub(crate) fn document_format(path: &Path) -> models::DocumentFormat {
    document_format_from_extension(path_extension_lower(path).as_deref())
}

fn textual_template_format(
    path: &Path,
) -> Result<textual_template::factory::TextualTemplateFormat, String> {
    let extension = path_extension_lower(path);
    match extension.as_deref() {
        Some("txt" | "md" | "markdown" | "tex" | "latex") | None => {
            Ok(format_from_extension(extension.as_deref()))
        }
        _ => Err(unsupported_textual_format_error(extension.as_deref())),
    }
}

fn format_from_extension(
    extension: Option<&str>,
) -> textual_template::factory::TextualTemplateFormat {
    match extension {
        Some("md" | "markdown") => textual_template::factory::TextualTemplateFormat::Markdown,
        Some("tex" | "latex") => textual_template::factory::TextualTemplateFormat::Tex,
        _ => textual_template::factory::TextualTemplateFormat::PlainText,
    }
}

fn document_format_from_extension(extension: Option<&str>) -> models::DocumentFormat {
    match extension {
        Some("docx") => models::DocumentFormat::Docx,
        Some("pdf") => models::DocumentFormat::Pdf,
        Some("md" | "markdown") => models::DocumentFormat::Markdown,
        Some("tex" | "latex") => models::DocumentFormat::Tex,
        _ => models::DocumentFormat::PlainText,
    }
}

fn unsupported_textual_format_error(extension: Option<&str>) -> String {
    format!(
        "暂不支持该文件格式：.{}。当前支持：{}。",
        extension.unwrap_or_default(),
        SUPPORTED_EXTENSIONS_HINT
    )
}

fn decode_utf16_payload(payload: &[u8], little_endian: bool) -> Result<String, String> {
    if !payload.len().is_multiple_of(2) {
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
