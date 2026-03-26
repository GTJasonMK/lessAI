use std::{fs, path::Path};

use uuid::Uuid;

use crate::{adapters, models};

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

pub(crate) struct LoadedDocumentSource {
    pub(crate) source_text: String,
    pub(crate) regions: Option<Vec<adapters::TextRegion>>,
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
            Ok(LoadedDocumentSource {
                source_text,
                regions: Some(regions),
            })
        }
        Some("doc") => {
            Err("暂不支持 .doc（老版 Word 二进制格式）。请另存为 .docx 后再导入。".to_string())
        }
        Some("pdf") => {
            let bytes = fs::read(path).map_err(|error| error.to_string())?;
            let source_text = adapters::pdf::PdfAdapter::extract_text(&bytes)?;
            Ok(LoadedDocumentSource {
                source_text,
                regions: None,
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
            Ok(LoadedDocumentSource {
                source_text,
                regions: None,
            })
        }
        None => {
            let bytes = fs::read(path).map_err(|error| error.to_string())?;
            let source_text = decode_text_file(&bytes)?;
            Ok(LoadedDocumentSource {
                source_text,
                regions: None,
            })
        }
    }
}

pub(crate) fn ensure_document_can_write_back(path: &str) -> Result<(), String> {
    let ext = path_extension_lower(Path::new(path)).unwrap_or_default();
    if ext == "docx" {
        return Err(
            "当前文件为 .docx：暂不支持写回覆盖（会破坏文件结构）。请使用“导出”为 .txt/.md 或另存为纯文本后再写回。"
                .to_string(),
        );
    }
    if ext == "pdf" {
        return Err("当前文件为 .pdf：暂不支持写回覆盖（PDF 不是纯文本格式）。请使用“导出”为 .txt 后再进行后续排版。".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_utf8_bom_text_file() {
        let bytes = [0xEF, 0xBB, 0xBF, b'a', b'b', b'c'];
        assert_eq!(decode_text_file(&bytes).unwrap(), "abc");
    }

    #[test]
    fn decode_utf16_le_bom_text_file() {
        // "A\n" in UTF-16LE with BOM
        let bytes = [0xFF, 0xFE, b'A', 0x00, b'\n', 0x00];
        assert_eq!(decode_text_file(&bytes).unwrap(), "A\n");
    }

    #[test]
    fn decode_utf16_be_bom_text_file() {
        // "A\n" in UTF-16BE with BOM
        let bytes = [0xFE, 0xFF, 0x00, b'A', 0x00, b'\n'];
        assert_eq!(decode_text_file(&bytes).unwrap(), "A\n");
    }

    #[test]
    fn decode_invalid_text_file_returns_error() {
        // Invalid UTF-8 and no BOM
        let bytes = [0xFF, 0xFF, 0xFF];
        assert!(decode_text_file(&bytes).is_err());
    }
}
