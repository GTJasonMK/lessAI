use std::io::{Cursor, Read};

use quick_xml::{events::Event, Reader};
use zip::ZipArchive;

/// Docx 适配器：从 `.docx`（Office Open XML）中抽取可改写的纯文本。
///
/// 重要说明：
/// - `.docx` 是 zip + XML 的二进制容器；当前阶段只支持“读取并抽取正文文本”。
/// - 抽取后的文本会进入既有的分块/改写/导出流程；
/// - **不支持写回覆盖 `.docx` 原文件**（否则会破坏文件结构）。
pub struct DocxAdapter;

impl DocxAdapter {
    pub fn extract_text(docx_bytes: &[u8]) -> Result<String, String> {
        if docx_bytes.is_empty() {
            return Err("docx 文件为空。".to_string());
        }

        let cursor = Cursor::new(docx_bytes);
        let mut archive = ZipArchive::new(cursor)
            .map_err(|error| format!("无法解析 docx（zip 结构错误）：{error}"))?;

        let mut file = archive
            .by_name("word/document.xml")
            .map_err(|_| "docx 缺少 word/document.xml，无法读取正文。".to_string())?;

        let mut xml = String::new();
        file.read_to_string(&mut xml)
            .map_err(|error| format!("读取 document.xml 失败：{error}"))?;

        extract_text_from_document_xml(&xml)
    }
}

fn local_name(name: &[u8]) -> &[u8] {
    match name.iter().rposition(|b| *b == b':') {
        Some(pos) if pos + 1 < name.len() => &name[pos + 1..],
        _ => name,
    }
}

fn extract_text_from_document_xml(xml: &str) -> Result<String, String> {
    let mut reader = Reader::from_str(xml);
    reader.trim_text(false);

    let mut buf = Vec::new();
    let mut paragraphs: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_text = false;
    let mut in_paragraph = false;
    let mut current_list_prefix = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match local_name(e.name().as_ref()) {
                b"p" => {
                    // 新段落开始：如果上一个段落意外未闭合，先收尾（容错）。
                    if in_paragraph {
                        paragraphs.push(apply_paragraph_prefix(
                            &current_list_prefix,
                            std::mem::take(&mut current),
                        ));
                    }
                    in_paragraph = true;
                    current.clear();
                    current_list_prefix.clear();
                }
                b"t" => in_text = true,
                b"numPr" => {
                    // 列表段落（最常见的 Word 项目符号/编号实现方式）
                    // 无法可靠还原真实编号/符号与缩进层级（需要解析 numbering.xml），
                    // 但至少输出一个 `- ` 前缀保留“列表语义”。
                    if in_paragraph && current_list_prefix.is_empty() {
                        current_list_prefix = "- ".to_string();
                    }
                }
                _ => {}
            },
            Ok(Event::End(e)) => match local_name(e.name().as_ref()) {
                b"t" => in_text = false,
                b"p" => {
                    if in_paragraph {
                        paragraphs.push(apply_paragraph_prefix(
                            &current_list_prefix,
                            std::mem::take(&mut current),
                        ));
                        in_paragraph = false;
                    }
                }
                _ => {}
            },
            Ok(Event::Empty(e)) => match local_name(e.name().as_ref()) {
                b"tab" => current.push('\t'),
                b"br" | b"cr" => current.push('\n'),
                _ => {}
            },
            Ok(Event::Text(e)) => {
                if in_text {
                    let text = e
                        .unescape()
                        .map_err(|error| format!("解析 document.xml 文本失败：{error}"))?;
                    current.push_str(&text);
                }
            }
            Ok(Event::CData(e)) => {
                if in_text {
                    let text = e
                        .unescape()
                        .map_err(|error| format!("解析 document.xml CDATA 失败：{error}"))?;
                    current.push_str(&text);
                }
            }
            Ok(Event::Eof) => break,
            Err(error) => return Err(format!("解析 document.xml 失败：{error}")),
            _ => {}
        }

        buf.clear();
    }

    // 兜底：未闭合段落也要收尾。
    if in_paragraph {
        paragraphs.push(apply_paragraph_prefix(&current_list_prefix, current));
    }

    // 段落间用空行分隔，既符合“自然文本”的阅读习惯，也便于后续 Paragraph preset 分块。
    let mut out = paragraphs.join("\n\n");
    out = out.trim_matches('\u{feff}').to_string();
    Ok(out)
}

fn apply_paragraph_prefix(prefix: &str, body: String) -> String {
    if prefix.is_empty() {
        return body;
    }
    if body.trim().is_empty() {
        return body;
    }
    if body.starts_with(prefix) {
        return body;
    }
    format!("{prefix}{body}")
}

#[cfg(test)]
mod tests {
    use super::DocxAdapter;
    use std::io::Write;
    use zip::{write::FileOptions, ZipWriter};

    fn build_minimal_docx(document_xml: &str) -> Vec<u8> {
        let mut out = Vec::new();
        let cursor = std::io::Cursor::new(&mut out);
        let mut zip = ZipWriter::new(cursor);
        let options = FileOptions::default();

        zip.start_file("word/document.xml", options)
            .expect("start file");
        zip.write_all(document_xml.as_bytes()).expect("write xml");
        zip.finish().expect("finish zip");
        out
    }

    #[test]
    fn extracts_plain_text_from_docx_document_xml() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>第一段</w:t></w:r></w:p>
    <w:p><w:r><w:t>第二段</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
        let bytes = build_minimal_docx(xml);
        let text = DocxAdapter::extract_text(&bytes).expect("extract text");
        assert_eq!(text, "第一段\n\n第二段");
    }

    #[test]
    fn preserves_tabs_and_line_breaks() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:t>a</w:t></w:r>
      <w:r><w:tab/></w:r>
      <w:r><w:t>b</w:t></w:r>
      <w:r><w:br/></w:r>
      <w:r><w:t>c</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
        let bytes = build_minimal_docx(xml);
        let text = DocxAdapter::extract_text(&bytes).expect("extract text");
        assert_eq!(text, "a\tb\nc");
    }

    #[test]
    fn keeps_empty_paragraphs_as_blank_lines() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p></w:p>
    <w:p><w:r><w:t>正文</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
        let bytes = build_minimal_docx(xml);
        let text = DocxAdapter::extract_text(&bytes).expect("extract text");
        assert_eq!(text, "\n\n正文");
    }

    #[test]
    fn adds_list_prefix_when_numpr_present() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:pPr><w:numPr><w:ilvl w:val="0"/></w:numPr></w:pPr>
      <w:r><w:t>第一项</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
        let bytes = build_minimal_docx(xml);
        let text = DocxAdapter::extract_text(&bytes).expect("extract text");
        assert_eq!(text, "- 第一项");
    }
}
