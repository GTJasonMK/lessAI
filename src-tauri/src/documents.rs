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

pub(crate) fn document_format(path: &Path) -> models::DocumentFormat {
    match path_extension_lower(path).as_deref() {
        Some("md") | Some("markdown") => models::DocumentFormat::Markdown,
        Some("tex") | Some("latex") => models::DocumentFormat::Tex,
        _ => models::DocumentFormat::PlainText,
    }
}

pub(crate) struct LoadedDocumentSource {
    pub(crate) source_text: String,
    pub(crate) regions: Option<Vec<adapters::TextRegion>>,
}

pub(crate) fn load_document_source(
    path: &Path,
    rewrite_headings: bool,
) -> Result<LoadedDocumentSource, String> {
    match path_extension_lower(path).as_deref() {
        Some("docx") => {
            let bytes = fs::read(path).map_err(|error| error.to_string())?;
            let regions = adapters::docx::DocxAdapter::extract_regions(&bytes, rewrite_headings)?;
            let source_text = regions
                .iter()
                .map(|region| region.body.as_str())
                .collect::<String>();
            Ok(LoadedDocumentSource {
                source_text,
                regions: Some(regions),
            })
        }
        Some("doc") => {
            Err("暂不支持 .doc（老版 Word 二进制格式）。请另存为 .docx 后再导入。".to_string())
        }
        _ => Ok(LoadedDocumentSource {
            source_text: fs::read_to_string(path).map_err(|error| error.to_string())?,
            regions: None,
        }),
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
    Ok(())
}
