use std::io::{Cursor, Read};

use quick_xml::{events::Event, Reader};
use zip::ZipArchive;

use super::TextRegion;

/// Docx 适配器：从 `.docx`（Office Open XML）中抽取可改写的纯文本。
///
/// 重要说明：
/// - `.docx` 是 zip + XML 的二进制容器；当前阶段只支持“读取并抽取正文文本”。
/// - 抽取后的文本会进入既有的分块/改写/导出流程；
/// - **不支持写回覆盖 `.docx` 原文件**（否则会破坏文件结构）。
pub struct DocxAdapter;

impl DocxAdapter {
    #[cfg(test)]
    pub fn extract_text(docx_bytes: &[u8]) -> Result<String, String> {
        let xml = read_document_xml(docx_bytes)?;
        extract_text_from_document_xml(&xml)
    }

    pub fn extract_regions(
        docx_bytes: &[u8],
        rewrite_headings: bool,
    ) -> Result<Vec<TextRegion>, String> {
        let xml = read_document_xml(docx_bytes)?;
        extract_regions_from_document_xml(&xml, rewrite_headings)
    }
}

fn read_document_xml(docx_bytes: &[u8]) -> Result<String, String> {
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

    Ok(xml)
}

fn local_name(name: &[u8]) -> &[u8] {
    match name.iter().rposition(|b| *b == b':') {
        Some(pos) if pos + 1 < name.len() => &name[pos + 1..],
        _ => name,
    }
}

#[derive(Debug, Clone)]
struct DocxParagraph {
    text: String,
    is_heading: bool,
}

#[cfg(test)]
fn extract_text_from_document_xml(xml: &str) -> Result<String, String> {
    let paragraphs = extract_paragraphs_from_document_xml(xml)?;
    let mut out = paragraphs
        .into_iter()
        .map(|paragraph| paragraph.text)
        .collect::<Vec<_>>()
        .join("\n\n");
    out = out.trim_matches('\u{feff}').to_string();
    Ok(out)
}

fn extract_regions_from_document_xml(
    xml: &str,
    rewrite_headings: bool,
) -> Result<Vec<TextRegion>, String> {
    let paragraphs = extract_paragraphs_from_document_xml(xml)?;
    if paragraphs.is_empty() {
        return Ok(Vec::new());
    }

    let mut out: Vec<TextRegion> = Vec::new();
    let total = paragraphs.len();
    for (index, paragraph) in paragraphs.into_iter().enumerate() {
        let mut body = paragraph.text;
        if index + 1 < total {
            body.push_str("\n\n");
        }
        if body.is_empty() {
            continue;
        }

        let skip_rewrite = paragraph.is_heading && !rewrite_headings;
        if let Some(last) = out.last_mut() {
            if last.skip_rewrite == skip_rewrite {
                last.body.push_str(&body);
                continue;
            }
        }
        out.push(TextRegion { body, skip_rewrite });
    }

    Ok(out)
}

fn is_heading_style_id(style_id: &str) -> bool {
    let lowered = style_id.trim().to_ascii_lowercase();
    lowered.starts_with("heading") || matches!(lowered.as_str(), "title" | "subtitle")
}

fn attr_value(bytes: &quick_xml::events::BytesStart<'_>, key: &[u8]) -> Option<String> {
    for attr in bytes.attributes().flatten() {
        if local_name(attr.key.as_ref()) != key {
            continue;
        }
        if let Ok(value) = attr.unescape_value() {
            return Some(value.into_owned());
        }
        if let Ok(value) = std::str::from_utf8(attr.value.as_ref()) {
            return Some(value.to_string());
        }
    }
    None
}

fn extract_paragraphs_from_document_xml(xml: &str) -> Result<Vec<DocxParagraph>, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);

    let mut buf = Vec::new();
    let mut paragraphs: Vec<DocxParagraph> = Vec::new();
    let mut current = String::new();
    let mut in_text = false;
    let mut in_paragraph = false;
    let mut current_list_prefix = String::new();
    let mut current_is_heading = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match local_name(e.name().as_ref()) {
                b"p" => {
                    // 新段落开始：如果上一个段落意外未闭合，先收尾（容错）。
                    if in_paragraph {
                        paragraphs.push(DocxParagraph {
                            text: apply_paragraph_prefix(
                                &current_list_prefix,
                                std::mem::take(&mut current),
                            ),
                            is_heading: current_is_heading,
                        });
                    }
                    in_paragraph = true;
                    current.clear();
                    current_list_prefix.clear();
                    current_is_heading = false;
                }
                b"t" => in_text = true,
                b"pStyle" => {
                    if in_paragraph {
                        if let Some(style_id) = attr_value(&e, b"val") {
                            if is_heading_style_id(&style_id) {
                                current_is_heading = true;
                            }
                        }
                    }
                }
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
                        paragraphs.push(DocxParagraph {
                            text: apply_paragraph_prefix(
                                &current_list_prefix,
                                std::mem::take(&mut current),
                            ),
                            is_heading: current_is_heading,
                        });
                        in_paragraph = false;
                    }
                }
                _ => {}
            },
            Ok(Event::Empty(e)) => match local_name(e.name().as_ref()) {
                b"tab" => current.push('\t'),
                b"br" | b"cr" => current.push('\n'),
                b"pStyle" => {
                    if in_paragraph {
                        if let Some(style_id) = attr_value(&e, b"val") {
                            if is_heading_style_id(&style_id) {
                                current_is_heading = true;
                            }
                        }
                    }
                }
                _ => {}
            },
            Ok(Event::Text(e)) => {
                if in_text {
                    let text = e
                        .decode()
                        .map_err(|error| format!("解析 document.xml 文本失败：{error}"))?;
                    current.push_str(&text);
                }
            }
            Ok(Event::CData(e)) => {
                if in_text {
                    let text = e
                        .decode()
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
        paragraphs.push(DocxParagraph {
            text: apply_paragraph_prefix(&current_list_prefix, current),
            is_heading: current_is_heading,
        });
    }

    // 去除 BOM（极少数文件会携带），避免影响后续分块与对比。
    if let Some(first) = paragraphs.first_mut() {
        first.text = first.text.trim_start_matches('\u{feff}').to_string();
    }
    if let Some(last) = paragraphs.last_mut() {
        last.text = last.text.trim_end_matches('\u{feff}').to_string();
    }

    Ok(coalesce_softwrapped_docx_paragraphs(paragraphs))
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

fn is_listish_paragraph(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with("- ") || trimmed.starts_with("• ")
}

fn last_significant_char(text: &str) -> Option<char> {
    let trimmed = text.trim_end();
    for ch in trimmed.chars().rev() {
        if ch.is_whitespace() {
            continue;
        }
        // 跳过常见的“句末引号/括号”，便于正确识别 `。”` 这类结尾。
        if matches!(
            ch,
            '”' | '"' | '’' | '\'' | ')' | '）' | ']' | '】' | '}' | '》' | '」' | '』'
        ) {
            continue;
        }
        return Some(ch);
    }
    None
}

fn is_cjk_char(ch: char) -> bool {
    matches!(
        ch,
        '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}'
    )
}

fn looks_like_cjk(text: &str) -> bool {
    text.chars().any(is_cjk_char)
}

fn ends_with_terminal_punct(text: &str) -> bool {
    let Some(ch) = last_significant_char(text) else {
        return true;
    };
    matches!(
        ch,
        '。' | '！' | '？' | '!' | '?' | '.' | '；' | ';' | '：' | ':'
    )
}

fn should_enable_softwrap_coalescing(paragraphs: &[DocxParagraph]) -> bool {
    // 只在“明显是按行断开”的 docx 上启用合并：
    // - PDF 转 Word 常把每一行都变成一个 <w:p>，导致段落级分块退化成“每行一个 chunk”；
    // - 正常 Word 文档中，段落数量较少且长度分布更离散，盲目合并容易误伤。
    let candidates = paragraphs
        .iter()
        .filter(|p| {
            let body = p.text.trim();
            !body.is_empty() && !p.is_heading && !is_listish_paragraph(body)
        })
        .collect::<Vec<_>>();
    let total = candidates.len();
    // 经验阈值：
    // - 太少的段落很难判断是否“按行断开”，合并容易误伤；
    // - 但现实里也会有只有几十行以内的短材料（作业/题目/通知）从 PDF 转 Word，
    //   如果阈值过高会导致完全不合并，段落级分块退化为“每行一个块”。
    if total < 8 {
        return false;
    }

    let short = candidates
        .iter()
        .filter(|p| p.text.trim().chars().count() <= 80)
        .count();

    short * 100 / total >= 75
}

fn should_merge_softwrap_boundary(
    last_line_text: &str,
    current: &DocxParagraph,
    next: &DocxParagraph,
) -> bool {
    if current.is_heading || next.is_heading {
        return false;
    }

    let prev = last_line_text.trim();
    let next_body = next.text.trim();
    if prev.is_empty() || next_body.is_empty() {
        return false;
    }
    if is_listish_paragraph(prev) || is_listish_paragraph(next_body) {
        return false;
    }

    // 关键启发式：上一行“看起来像被硬换行截断”的概率较高，才合并到下一行。
    let prev_len = prev.chars().count();
    // 中文/日文这类无空格语言的“软换行行长度”通常更短；
    // 若阈值过高，会导致软换行合并中途断开，段落仍然过碎。
    let min_line_chars: usize = if looks_like_cjk(prev) { 6 } else { 12 };
    const MAX_LINE_CHARS: usize = 140;
    if prev_len < min_line_chars || prev_len > MAX_LINE_CHARS {
        return false;
    }
    if ends_with_terminal_punct(prev) {
        return false;
    }

    true
}

fn merge_softwrap_line(mut left: String, right: &str) -> String {
    let right = right.trim_start();
    if right.is_empty() {
        return left;
    }
    if left.trim().is_empty() {
        return right.to_string();
    }

    let left_last = left.trim_end().chars().last();
    let right_first = right.chars().next();

    // 英文断词：`exam-` + `ple` => `example`
    if left_last == Some('-') && right_first.is_some_and(|ch| ch.is_ascii_alphabetic()) {
        let trimmed = left.trim_end().to_string();
        let mut chars = trimmed.chars().collect::<Vec<_>>();
        if chars.last() == Some(&'-') {
            chars.pop();
        }
        let rebuilt = chars.into_iter().collect::<String>();
        return format!("{rebuilt}{right}");
    }

    let need_space = left_last.is_some_and(|ch| ch.is_ascii_alphanumeric())
        && right_first.is_some_and(|ch| ch.is_ascii_alphanumeric());

    let left_ends_with_ws = left.chars().last().is_some_and(|ch| ch.is_whitespace());
    if need_space && !left_ends_with_ws {
        left.push(' ');
    }
    left.push_str(right);
    left
}

fn coalesce_softwrapped_docx_paragraphs(paragraphs: Vec<DocxParagraph>) -> Vec<DocxParagraph> {
    if paragraphs.is_empty() {
        return paragraphs;
    }
    if !should_enable_softwrap_coalescing(&paragraphs) {
        return paragraphs;
    }

    let mut out: Vec<DocxParagraph> = Vec::new();
    let mut current: Option<DocxParagraph> = None;
    // 用“上一行原始文本”做边界判断，避免合并后长度变长导致提前终止。
    let mut last_line_text = String::new();

    for paragraph in paragraphs.into_iter() {
        if current.is_none() {
            last_line_text = paragraph.text.clone();
            current = Some(paragraph);
            continue;
        }

        let mut cur = current.take().unwrap();
        if should_merge_softwrap_boundary(&last_line_text, &cur, &paragraph) {
            cur.text = merge_softwrap_line(cur.text, &paragraph.text);
            last_line_text = paragraph.text.clone();
            current = Some(cur);
            continue;
        }

        out.push(cur);
        last_line_text = paragraph.text.clone();
        current = Some(paragraph);
    }

    if let Some(cur) = current.take() {
        out.push(cur);
    }

    out
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
        let options = FileOptions::<()>::default();

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

    #[test]
    fn marks_heading_styles_as_skip_regions_by_default() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:pPr><w:pStyle w:val="Heading1"/></w:pPr>
      <w:r><w:t>标题</w:t></w:r>
    </w:p>
    <w:p><w:r><w:t>正文</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
        let bytes = build_minimal_docx(xml);

        let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");
        assert!(regions
            .iter()
            .any(|region| region.skip_rewrite && region.body.contains("标题")));

        let rebuilt = regions
            .iter()
            .map(|region| region.body.as_str())
            .collect::<String>();
        let text = DocxAdapter::extract_text(&bytes).expect("extract text");
        assert_eq!(rebuilt, text);
    }

    #[test]
    fn allows_heading_styles_to_be_rewritten_when_enabled() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:pPr><w:pStyle w:val="Title"/></w:pPr>
      <w:r><w:t>文档标题</w:t></w:r>
    </w:p>
    <w:p><w:r><w:t>正文</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
        let bytes = build_minimal_docx(xml);

        let regions = DocxAdapter::extract_regions(&bytes, true).expect("extract regions");
        assert!(regions
            .iter()
            .any(|region| !region.skip_rewrite && region.body.contains("文档标题")));

        let rebuilt = regions
            .iter()
            .map(|region| region.body.as_str())
            .collect::<String>();
        let text = DocxAdapter::extract_text(&bytes).expect("extract text");
        assert_eq!(rebuilt, text);
    }

    #[test]
    fn coalesces_softwrapped_paragraph_runs_when_document_looks_line_wrapped() {
        // 模拟“PDF 转 Word：每行都是一个段落”的情况。
        // 预期：导入到工作台后应尽量还原为更少的自然段，避免段落级分块过碎。
        let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>这一段被硬换行拆成很多行</w:t></w:r></w:p>
    <w:p><w:r><w:t>每行都成了一个段落导致切块过碎</w:t></w:r></w:p>
    <w:p><w:r><w:t>导入时需要做轻量合并</w:t></w:r></w:p>
    <w:p><w:r><w:t>否则连一句完整的话都不在同一块里</w:t></w:r></w:p>
    <w:p><w:r><w:t>这里继续补一些行以触发启发式</w:t></w:r></w:p>
    <w:p><w:r><w:t>第六行内容用于模拟真实文档</w:t></w:r></w:p>
    <w:p><w:r><w:t>第七行内容用于模拟真实文档</w:t></w:r></w:p>
    <w:p><w:r><w:t>第八行内容用于模拟真实文档</w:t></w:r></w:p>
    <w:p><w:r><w:t>第九行内容用于模拟真实文档</w:t></w:r></w:p>
    <w:p><w:r><w:t>第十行内容用于模拟真实文档</w:t></w:r></w:p>
    <w:p><w:r><w:t>第十一行内容用于模拟真实文档</w:t></w:r></w:p>
    <w:p><w:r><w:t>最后一行收尾。</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
        let bytes = build_minimal_docx(xml);
        let text = DocxAdapter::extract_text(&bytes).expect("extract text");
        // 合并后应只剩 1 个自然段（没有段落空行分隔符）。
        assert!(!text.contains("\n\n"));
        assert!(text.contains("最后一行收尾。"));
    }

    #[test]
    fn coalesces_softwrapped_paragraph_runs_when_document_looks_line_wrapped_with_fewer_lines() {
        // 现实里也会出现“只有几行”的 PDF→Word 文档（例如作业/短材料）；
        // 仍然应触发软换行合并，否则段落级 chunk 会退化为“每行一个块”。
        let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>这一段被硬换行拆成很多行</w:t></w:r></w:p>
    <w:p><w:r><w:t>每行都成了一个段落导致切块过碎</w:t></w:r></w:p>
    <w:p><w:r><w:t>导入时需要做轻量合并</w:t></w:r></w:p>
    <w:p><w:r><w:t>否则连一句完整的话都不在同一块里</w:t></w:r></w:p>
    <w:p><w:r><w:t>这里继续补一些行以触发启发式</w:t></w:r></w:p>
    <w:p><w:r><w:t>第六行内容用于模拟真实文档</w:t></w:r></w:p>
    <w:p><w:r><w:t>第七行内容用于模拟真实文档</w:t></w:r></w:p>
    <w:p><w:r><w:t>第八行内容用于模拟真实文档</w:t></w:r></w:p>
    <w:p><w:r><w:t>第九行内容用于模拟真实文档</w:t></w:r></w:p>
    <w:p><w:r><w:t>最后一行收尾。</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
        let bytes = build_minimal_docx(xml);
        let text = DocxAdapter::extract_text(&bytes).expect("extract text");
        assert!(!text.contains("\n\n"));
        assert!(text.contains("最后一行收尾。"));
    }
}
