use std::{
    collections::HashMap,
    io::{Cursor, Read, Write},
};

use quick_xml::{
    events::{BytesEnd, BytesStart, BytesText, Event},
    Reader, Writer,
};
use zip::{write::FileOptions, ZipArchive, ZipWriter};

use super::{
    model::{
        EditableRegionRender, EditableRegionTemplate, LockedRegionRender, LockedRegionTemplate,
        WritebackBlockTemplate, WritebackParagraphTemplate, WritebackRegionTemplate,
    },
    placeholders,
};
use crate::{
    adapters::TextRegion,
    models::{ChunkPresentation, DiffType},
};

/// Docx 适配器：从 `.docx`（Office Open XML）中抽取可改写的纯文本。
///
/// 重要说明：
/// - `.docx` 是 zip + XML 的二进制容器；
/// - 当前仅支持“简单 docx”：正文只包含普通段落/标题；
/// - 检测到复杂结构时会直接报错，不会带着不确定语义继续写回。
pub struct DocxAdapter;

impl DocxAdapter {
    #[cfg(test)]
    pub fn extract_text(docx_bytes: &[u8]) -> Result<String, String> {
        let xml = read_document_xml(docx_bytes)?;
        extract_text_from_document_xml(&xml)
    }

    pub(crate) fn extract_writeback_source_text(docx_bytes: &[u8]) -> Result<String, String> {
        let xml = read_document_xml(docx_bytes)?;
        let hyperlink_targets = read_document_relationships(docx_bytes)?;
        let blocks = extract_writeback_paragraph_templates(&xml, &hyperlink_targets)?;
        Ok(build_writeback_source_text(&blocks))
    }

    #[cfg(test)]
    pub fn extract_writeback_regions(docx_bytes: &[u8]) -> Result<Vec<TextRegion>, String> {
        let xml = read_document_xml(docx_bytes)?;
        let hyperlink_targets = read_document_relationships(docx_bytes)?;
        let blocks = extract_writeback_paragraph_templates(&xml, &hyperlink_targets)?;
        Ok(flatten_writeback_blocks_for_test(&blocks))
    }

    pub fn extract_regions(
        docx_bytes: &[u8],
        rewrite_headings: bool,
    ) -> Result<Vec<TextRegion>, String> {
        let xml = read_document_xml(docx_bytes)?;
        let hyperlink_targets = read_document_relationships(docx_bytes)?;
        extract_regions_from_document_xml(&xml, &hyperlink_targets, rewrite_headings)
    }

    pub fn write_updated_text(
        docx_bytes: &[u8],
        expected_source_text: &str,
        updated_text: &str,
    ) -> Result<Vec<u8>, String> {
        let xml = read_document_xml(docx_bytes)?;
        let hyperlink_targets = read_document_relationships(docx_bytes)?;
        let blocks = extract_writeback_paragraph_templates(&xml, &hyperlink_targets)?;
        let current_source_text = build_writeback_source_text(&blocks);
        if current_source_text != expected_source_text {
            return Err(
                "docx 原文件内容与当前会话不一致，文件可能已在外部发生变化。为避免误写，请重新导入。"
                    .to_string(),
            );
        }

        let updated_regions = build_plain_text_editor_updated_regions(&blocks, updated_text)?;
        let updated_xml = rewrite_document_xml_with_regions(&xml, &blocks, &updated_regions)?;
        replace_document_xml(docx_bytes, &updated_xml)
    }

    pub fn write_updated_regions(
        docx_bytes: &[u8],
        expected_source_text: &str,
        updated_regions: &[TextRegion],
    ) -> Result<Vec<u8>, String> {
        let xml = read_document_xml(docx_bytes)?;
        let hyperlink_targets = read_document_relationships(docx_bytes)?;
        let paragraphs = extract_writeback_paragraph_templates(&xml, &hyperlink_targets)?;
        let current_source_text = build_writeback_source_text(&paragraphs);
        if current_source_text != expected_source_text {
            return Err(
                "docx 原文件内容与当前会话不一致，文件可能已在外部发生变化。为避免误写，请重新导入。"
                    .to_string(),
            );
        }

        let updated_xml = rewrite_document_xml_with_regions(&xml, &paragraphs, updated_regions)?;
        replace_document_xml(docx_bytes, &updated_xml)
    }

    pub fn validate_writeback(docx_bytes: &[u8]) -> Result<(), String> {
        let xml = read_document_xml(docx_bytes)?;
        let hyperlink_targets = read_document_relationships(docx_bytes)?;
        extract_writeback_paragraph_templates(&xml, &hyperlink_targets).map(|_| ())
    }

    pub fn validate_plain_text_editor(docx_bytes: &[u8]) -> Result<(), String> {
        Self::validate_writeback(docx_bytes)
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

fn read_document_relationships(docx_bytes: &[u8]) -> Result<HashMap<String, String>, String> {
    let cursor = Cursor::new(docx_bytes);
    let mut archive = ZipArchive::new(cursor)
        .map_err(|error| format!("无法解析 docx（zip 结构错误）：{error}"))?;
    let mut file = match archive.by_name("word/_rels/document.xml.rels") {
        Ok(file) => file,
        Err(_) => return Ok(HashMap::new()),
    };

    let mut xml = String::new();
    file.read_to_string(&mut xml)
        .map_err(|error| format!("读取 document.xml.rels 失败：{error}"))?;
    parse_relationship_targets(&xml)
}

fn parse_relationship_targets(xml: &str) -> Result<HashMap<String, String>, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut targets = HashMap::new();

    loop {
        let event = match reader.read_event_into(&mut buf) {
            Ok(event) => event.into_owned(),
            Err(error) => return Err(format!("解析 document.xml.rels 失败：{error}")),
        };

        match event {
            Event::Start(e) | Event::Empty(e) => {
                if local_name(e.name().as_ref()) != b"Relationship" {
                    buf.clear();
                    continue;
                }
                let relationship_type = attr_value(&e, b"Type");
                if !relationship_type
                    .as_deref()
                    .is_some_and(|value| value.ends_with("/hyperlink"))
                {
                    buf.clear();
                    continue;
                }
                let id = attr_value(&e, b"Id")
                    .ok_or_else(|| "document.xml.rels 中的超链接关系缺少 Id。".to_string())?;
                let target = attr_value(&e, b"Target").ok_or_else(|| {
                    format!("document.xml.rels 中的超链接关系 {id} 缺少 Target。")
                })?;
                targets.insert(id, target);
            }
            Event::Eof => break,
            _ => {}
        }

        buf.clear();
    }

    Ok(targets)
}

fn local_name(name: &[u8]) -> &[u8] {
    match name.iter().rposition(|b| *b == b':') {
        Some(pos) if pos + 1 < name.len() => &name[pos + 1..],
        _ => name,
    }
}

fn local_name_owned(name: &[u8]) -> Vec<u8> {
    local_name(name).to_vec()
}

const DOCX_BLOCK_SEPARATOR: &str = "\n\n";
const DOCX_PAGE_BREAK_PLACEHOLDER: &str = "[分页符]";
const DOCX_EMBEDDED_OBJECT_ERROR: &str =
    "当前不支持包含嵌入 Office 对象的 docx（例如 OLE、图表或 SmartArt）。";
const DOCX_HYPERLINK_PAGE_BREAK_ERROR: &str =
    "当前不支持超链接内分页符的 docx：这类结构无法安全写回，请先在 Word 中调整后再导入。";

#[derive(Debug, Clone)]
struct DocxParagraph {
    text: String,
    is_heading: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct RunStyle {
    bold: bool,
    italic: bool,
    underline: bool,
}

#[cfg(test)]
fn extract_text_from_document_xml(xml: &str) -> Result<String, String> {
    let regions = extract_regions_from_document_xml(xml, &HashMap::new(), true)?;
    Ok(regions
        .into_iter()
        .map(|region| region.body)
        .collect::<String>()
        .trim_matches('\u{feff}')
        .to_string())
}

fn extract_regions_from_document_xml(
    xml: &str,
    hyperlink_targets: &HashMap<String, String>,
    rewrite_headings: bool,
) -> Result<Vec<TextRegion>, String> {
    let (blocks, _paragraphs) =
        extract_import_blocks_from_document_xml(xml, hyperlink_targets, rewrite_headings)?;
    Ok(flatten_import_blocks(blocks))
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

fn toggle_attr_enabled(event: &BytesStart<'_>) -> bool {
    !matches!(
        attr_value(event, b"val")
            .as_deref()
            .map(|value| value.trim().to_ascii_lowercase()),
        Some(value) if matches!(value.as_str(), "0" | "false" | "off" | "none")
    )
}

fn underline_enabled(event: &BytesStart<'_>) -> bool {
    !matches!(
        attr_value(event, b"val")
            .as_deref()
            .map(|value| value.trim().to_ascii_lowercase()),
        Some(value) if value == "none"
    )
}

fn hyperlink_target(
    event: &BytesStart<'_>,
    hyperlink_targets: &HashMap<String, String>,
) -> Option<String> {
    attr_value(event, b"id")
        .and_then(|id| hyperlink_targets.get(&id).cloned())
        .or_else(|| attr_value(event, b"anchor").map(|anchor| format!("#{anchor}")))
}

fn extract_import_blocks_from_document_xml(
    xml: &str,
    hyperlink_targets: &HashMap<String, String>,
    rewrite_headings: bool,
) -> Result<(Vec<Vec<TextRegion>>, Vec<DocxParagraph>), String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);

    let mut buf = Vec::new();
    let mut body_depth = 0usize;
    let mut paragraph_depth = 0usize;
    let mut table_depth = 0usize;
    let mut sdt_depth = 0usize;
    let mut paragraph_events: Vec<Event<'static>> = Vec::new();
    let mut blocks: Vec<Vec<TextRegion>> = Vec::new();
    let mut paragraphs: Vec<DocxParagraph> = Vec::new();

    loop {
        let event = match reader.read_event_into(&mut buf) {
            Ok(event) => event.into_owned(),
            Err(error) => return Err(format!("解析 document.xml 失败：{error}")),
        };

        match event {
            Event::Start(e) => handle_import_start(
                e,
                &mut body_depth,
                &mut paragraph_depth,
                &mut table_depth,
                &mut sdt_depth,
                &mut paragraph_events,
                &mut blocks,
            )?,
            Event::Empty(e) => handle_import_empty(
                e,
                &mut body_depth,
                &mut paragraph_depth,
                &mut table_depth,
                &mut sdt_depth,
                &mut paragraph_events,
                &mut blocks,
                &mut paragraphs,
                hyperlink_targets,
                rewrite_headings,
            )?,
            Event::End(e) => handle_import_end(
                e,
                &mut body_depth,
                &mut paragraph_depth,
                &mut table_depth,
                &mut sdt_depth,
                &mut paragraph_events,
                &mut blocks,
                &mut paragraphs,
                hyperlink_targets,
                rewrite_headings,
            )?,
            Event::Text(_)
            | Event::CData(_)
            | Event::Comment(_)
            | Event::Decl(_)
            | Event::PI(_)
            | Event::DocType(_)
            | Event::GeneralRef(_) => {
                if paragraph_depth > 0 {
                    paragraph_events.push(event);
                }
            }
            Event::Eof => break,
        }

        buf.clear();
    }

    trim_paragraph_bom(&mut paragraphs);
    trim_region_bom(&mut blocks);
    Ok((blocks, paragraphs))
}

fn handle_import_start(
    event: BytesStart<'static>,
    body_depth: &mut usize,
    paragraph_depth: &mut usize,
    table_depth: &mut usize,
    sdt_depth: &mut usize,
    paragraph_events: &mut Vec<Event<'static>>,
    blocks: &mut Vec<Vec<TextRegion>>,
) -> Result<(), String> {
    let name = local_name_owned(event.name().as_ref());
    if *paragraph_depth > 0 {
        *paragraph_depth += 1;
        paragraph_events.push(Event::Start(event));
        return Ok(());
    }
    if *table_depth > 0 {
        *table_depth += 1;
        return Ok(());
    }
    if *sdt_depth > 0 {
        *sdt_depth += 1;
        return Ok(());
    }
    if name.as_slice() == b"body" {
        *body_depth = 1;
        return Ok(());
    }
    if *body_depth == 0 {
        return Ok(());
    }
    if *body_depth != 1 {
        *body_depth += 1;
        return Ok(());
    }

    match name.as_slice() {
        b"p" => {
            *paragraph_depth = 1;
            paragraph_events.clear();
            paragraph_events.push(Event::Start(event));
        }
        b"tbl" => *table_depth = 1,
        b"sdt" => *sdt_depth = 1,
        b"sectPr" => {
            blocks.push(vec![placeholders::locked_region(
                placeholders::DOCX_SECTION_BREAK_PLACEHOLDER,
                "section-break",
            )]);
            *body_depth += 1;
        }
        _ => {
            return Err(format!(
                "当前仅支持文档正文内容导入：检测到不支持的正文结构 <{}>。",
                tag_name(name.as_slice())
            ))
        }
    }
    Ok(())
}

fn handle_import_empty(
    event: BytesStart<'static>,
    body_depth: &mut usize,
    paragraph_depth: &mut usize,
    table_depth: &mut usize,
    sdt_depth: &mut usize,
    paragraph_events: &mut Vec<Event<'static>>,
    blocks: &mut Vec<Vec<TextRegion>>,
    paragraphs: &mut Vec<DocxParagraph>,
    hyperlink_targets: &HashMap<String, String>,
    rewrite_headings: bool,
) -> Result<(), String> {
    let name = local_name_owned(event.name().as_ref());
    if *paragraph_depth > 0 {
        paragraph_events.push(Event::Empty(event));
        return Ok(());
    }
    if *table_depth > 0 {
        return Ok(());
    }
    if *sdt_depth > 0 {
        return Ok(());
    }
    if *body_depth == 0 || *body_depth != 1 {
        return Ok(());
    }
    match name.as_slice() {
        b"p" => push_import_paragraph_block(
            vec![Event::Empty(event)],
            blocks,
            paragraphs,
            hyperlink_targets,
            rewrite_headings,
        ),
        b"tbl" => {
            blocks.push(vec![placeholders::locked_region(
                placeholders::DOCX_TABLE_PLACEHOLDER,
                "table",
            )]);
            Ok(())
        }
        b"sdt" => {
            blocks.push(vec![placeholders::locked_region(
                placeholders::DOCX_TOC_PLACEHOLDER,
                "toc",
            )]);
            Ok(())
        }
        b"sectPr" => {
            blocks.push(vec![placeholders::locked_region(
                placeholders::DOCX_SECTION_BREAK_PLACEHOLDER,
                "section-break",
            )]);
            Ok(())
        }
        _ => Err(format!(
            "当前仅支持文档正文内容导入：检测到不支持的正文结构 <{}>。",
            tag_name(name.as_slice())
        )),
    }
}

fn handle_import_end(
    event: BytesEnd<'static>,
    body_depth: &mut usize,
    paragraph_depth: &mut usize,
    table_depth: &mut usize,
    sdt_depth: &mut usize,
    paragraph_events: &mut Vec<Event<'static>>,
    blocks: &mut Vec<Vec<TextRegion>>,
    paragraphs: &mut Vec<DocxParagraph>,
    hyperlink_targets: &HashMap<String, String>,
    rewrite_headings: bool,
) -> Result<(), String> {
    let name = local_name_owned(event.name().as_ref());
    if *paragraph_depth > 0 {
        paragraph_events.push(Event::End(event));
        *paragraph_depth -= 1;
        if *paragraph_depth == 0 {
            let events = std::mem::take(paragraph_events);
            push_import_paragraph_block(
                events,
                blocks,
                paragraphs,
                hyperlink_targets,
                rewrite_headings,
            )?;
        }
        return Ok(());
    }
    if *table_depth > 0 {
        *table_depth -= 1;
        if *table_depth == 0 {
            blocks.push(vec![placeholders::locked_region(
                placeholders::DOCX_TABLE_PLACEHOLDER,
                "table",
            )]);
        }
        return Ok(());
    }
    if *sdt_depth > 0 {
        *sdt_depth -= 1;
        if *sdt_depth == 0 {
            blocks.push(vec![placeholders::locked_region(
                placeholders::DOCX_TOC_PLACEHOLDER,
                "toc",
            )]);
        }
        return Ok(());
    }
    if *body_depth == 0 {
        return Ok(());
    }
    if name.as_slice() == b"body" && *body_depth == 1 {
        *body_depth = 0;
    } else {
        *body_depth -= 1;
    }
    Ok(())
}

fn push_import_paragraph_block(
    events: Vec<Event<'static>>,
    blocks: &mut Vec<Vec<TextRegion>>,
    paragraphs: &mut Vec<DocxParagraph>,
    hyperlink_targets: &HashMap<String, String>,
    rewrite_headings: bool,
) -> Result<(), String> {
    let (regions, paragraph) =
        parse_import_paragraph(&events, hyperlink_targets, rewrite_headings)?;
    blocks.push(regions);
    paragraphs.push(paragraph);
    Ok(())
}

fn parse_import_paragraph(
    events: &[Event<'static>],
    hyperlink_targets: &HashMap<String, String>,
    rewrite_headings: bool,
) -> Result<(Vec<TextRegion>, DocxParagraph), String> {
    let mut regions: Vec<TextRegion> = Vec::new();
    let mut editable = String::new();
    let mut editable_presentation: Option<ChunkPresentation> = None;
    let mut formula = String::new();
    let mut is_heading = false;
    let mut paragraph_started = false;
    let mut paragraph_finished = false;
    let mut ppr_depth = 0usize;
    let mut run_depth = 0usize;
    let mut run_style_depth = 0usize;
    let mut in_text = false;
    let mut math_depth = 0usize;
    let mut math_text_depth = 0usize;
    let mut hyperlink_depth = 0usize;
    let mut current_run_style = RunStyle::default();
    let mut current_run_property_events: Vec<Event<'static>> = Vec::new();
    let mut current_hyperlink_target: Option<String> = None;
    let mut current_hyperlink_signature: Option<String> = None;
    let mut locked_inline_depth = 0usize;
    let mut locked_inline_events: Vec<Event<'static>> = Vec::new();
    let mut ignored_depth = 0usize;

    for event in events {
        if try_skip_ignored_paragraph_event(event, &mut ignored_depth) {
            continue;
        }
        if try_capture_locked_inline_object_event(
            event,
            run_style_depth,
            ppr_depth,
            math_depth,
            in_text,
            &mut locked_inline_depth,
            &mut locked_inline_events,
            &mut editable,
            &mut editable_presentation,
            &mut regions,
        )? {
            continue;
        }
        match event {
            Event::Start(e) => handle_paragraph_start(
                e,
                hyperlink_targets,
                &mut paragraph_started,
                &mut ppr_depth,
                &mut run_depth,
                &mut run_style_depth,
                &mut in_text,
                &mut math_depth,
                &mut math_text_depth,
                &mut hyperlink_depth,
                &mut current_run_style,
                &mut current_run_property_events,
                &mut current_hyperlink_target,
                &mut current_hyperlink_signature,
                &mut is_heading,
                &mut editable,
                &mut editable_presentation,
                &mut regions,
            )?,
            Event::Empty(e) => handle_paragraph_empty(
                e,
                &mut paragraph_started,
                &mut paragraph_finished,
                &mut ppr_depth,
                &mut run_depth,
                &mut run_style_depth,
                &mut hyperlink_depth,
                &mut current_run_style,
                &mut current_run_property_events,
                &mut current_hyperlink_target,
                &mut current_hyperlink_signature,
                &mut is_heading,
                &mut editable,
                &mut editable_presentation,
                &mut regions,
            )?,
            Event::End(e) => handle_paragraph_end(
                e,
                &mut paragraph_finished,
                &mut ppr_depth,
                &mut run_depth,
                &mut run_style_depth,
                &mut in_text,
                &mut math_depth,
                &mut math_text_depth,
                &mut formula,
                &mut hyperlink_depth,
                &mut current_hyperlink_target,
                &mut current_hyperlink_signature,
                &mut current_run_property_events,
                &mut regions,
            )?,
            Event::Text(e) => append_paragraph_text(
                e.decode()
                    .map_err(|error| format!("解析 document.xml 文本失败：{error}"))?
                    .as_ref(),
                in_text,
                math_text_depth > 0,
                &mut editable,
                &mut editable_presentation,
                current_editable_presentation(
                    &current_run_style,
                    current_hyperlink_target.clone(),
                    current_run_writeback_key(
                        &current_run_property_events,
                        current_hyperlink_signature.as_deref(),
                    ),
                ),
                &mut formula,
                &mut regions,
            )?,
            Event::CData(e) => append_paragraph_text(
                e.decode()
                    .map_err(|error| format!("解析 document.xml CDATA 失败：{error}"))?
                    .as_ref(),
                in_text,
                math_text_depth > 0,
                &mut editable,
                &mut editable_presentation,
                current_editable_presentation(
                    &current_run_style,
                    current_hyperlink_target.clone(),
                    current_run_writeback_key(
                        &current_run_property_events,
                        current_hyperlink_signature.as_deref(),
                    ),
                ),
                &mut formula,
                &mut regions,
            )?,
            Event::Comment(_)
            | Event::Decl(_)
            | Event::PI(_)
            | Event::DocType(_)
            | Event::GeneralRef(_)
            | Event::Eof => {}
        }
    }

    if !paragraph_started || !paragraph_finished {
        return Err("解析 docx 段落失败：段落未正常闭合。".to_string());
    }

    flush_editable_region(
        &mut regions,
        &mut editable,
        &mut editable_presentation,
        false,
    );
    flush_locked_region(&mut regions, &mut formula, "formula");
    if regions.is_empty() {
        regions.push(empty_region(is_heading && !rewrite_headings));
    } else if is_heading && !rewrite_headings {
        mark_all_regions_locked(&mut regions);
    }

    let text = regions
        .iter()
        .map(|region| region.body.as_str())
        .collect::<String>();
    Ok((regions, DocxParagraph { text, is_heading }))
}

fn try_skip_ignored_paragraph_event(event: &Event<'static>, ignored_depth: &mut usize) -> bool {
    if *ignored_depth > 0 {
        match event {
            Event::Start(_) => *ignored_depth += 1,
            Event::End(_) => *ignored_depth -= 1,
            _ => {}
        }
        return true;
    }

    match event {
        Event::Start(e) if is_ignorable_paragraph_name(local_name(e.name().as_ref())) => {
            *ignored_depth = 1;
            true
        }
        Event::Empty(e) if is_ignorable_paragraph_name(local_name(e.name().as_ref())) => true,
        _ => false,
    }
}

fn is_ignorable_paragraph_name(name: &[u8]) -> bool {
    matches!(
        name,
        b"bookmarkStart" | b"bookmarkEnd" | b"proofErr" | b"fldChar" | b"instrText"
    )
}

fn try_capture_locked_inline_object_event(
    event: &Event<'static>,
    run_style_depth: usize,
    ppr_depth: usize,
    math_depth: usize,
    in_text: bool,
    locked_inline_depth: &mut usize,
    locked_inline_events: &mut Vec<Event<'static>>,
    editable: &mut String,
    editable_presentation: &mut Option<ChunkPresentation>,
    regions: &mut Vec<TextRegion>,
) -> Result<bool, String> {
    if *locked_inline_depth > 0 {
        locked_inline_events.push(event.clone());
        match event {
            Event::Start(_) => *locked_inline_depth += 1,
            Event::End(_) => {
                *locked_inline_depth -= 1;
                if *locked_inline_depth == 0 {
                    push_locked_inline_object_region(regions, locked_inline_events)?;
                    locked_inline_events.clear();
                }
            }
            _ => {}
        }
        return Ok(true);
    }

    if !should_capture_locked_inline_object(event, run_style_depth, ppr_depth, math_depth, in_text)
    {
        return Ok(false);
    }

    flush_editable_region(regions, editable, editable_presentation, false);
    match event {
        Event::Start(_) => {
            *locked_inline_depth = 1;
            locked_inline_events.clear();
            locked_inline_events.push(event.clone());
        }
        Event::Empty(_) => push_locked_inline_object_region(regions, std::slice::from_ref(event))?,
        _ => return Err("解析 docx 段落失败：非法的锁定对象起点。".to_string()),
    }
    Ok(true)
}

fn should_capture_locked_inline_object(
    event: &Event<'static>,
    run_style_depth: usize,
    ppr_depth: usize,
    math_depth: usize,
    in_text: bool,
) -> bool {
    if in_text || run_style_depth > 0 || ppr_depth > 0 || math_depth > 0 {
        return false;
    }
    match event {
        Event::Start(e) | Event::Empty(e) => {
            is_locked_inline_object_name(local_name(e.name().as_ref()))
        }
        _ => false,
    }
}

fn is_locked_inline_object_name(name: &[u8]) -> bool {
    matches!(name, b"drawing" | b"pict" | b"AlternateContent")
}

fn push_locked_inline_object_region(
    regions: &mut Vec<TextRegion>,
    events: &[Event<'static>],
) -> Result<(), String> {
    let (text, kind) = classify_locked_object_placeholder(events)?;
    push_import_region(
        regions,
        text.to_string(),
        true,
        placeholders::placeholder_presentation(kind),
    );
    Ok(())
}

fn classify_locked_object_placeholder(
    events: &[Event<'static>],
) -> Result<(&'static str, &'static str), String> {
    if contains_local_tag(events, b"txbxContent") || contains_local_tag(events, b"textbox") {
        return Ok((placeholders::DOCX_TEXTBOX_PLACEHOLDER, "textbox"));
    }
    if contains_local_tag(events, b"pic") {
        return Ok((placeholders::DOCX_IMAGE_PLACEHOLDER, "image"));
    }
    if contains_local_tag(events, b"chart") {
        return Ok((placeholders::DOCX_CHART_PLACEHOLDER, "chart"));
    }
    if contains_local_tag(events, b"wgp") || contains_local_tag(events, b"grpSp") {
        return Ok((placeholders::DOCX_GROUP_SHAPE_PLACEHOLDER, "group-shape"));
    }
    if is_vml_shape_object(events)
        || contains_local_tag(events, b"wsp")
        || contains_local_tag(events, b"sp")
        || contains_local_tag(events, b"cxnSp")
        || contains_local_tag(events, b"graphicFrame")
        || contains_local_tag(events, b"relIds")
        || contains_local_tag(events, b"dataModelExt")
    {
        return Ok((placeholders::DOCX_SHAPE_PLACEHOLDER, "shape"));
    }
    Err("当前仅支持文章语义相关的 docx：无法归类正文中的图形对象，无法安全导入。".to_string())
}

fn is_vml_shape_object(events: &[Event<'static>]) -> bool {
    contains_local_tag(events, b"pict")
        && (contains_local_tag(events, b"rect")
            || contains_local_tag(events, b"roundrect")
            || contains_local_tag(events, b"shape")
            || contains_local_tag(events, b"shapetype")
            || contains_local_tag(events, b"line")
            || contains_local_tag(events, b"oval"))
}

fn contains_local_tag(events: &[Event<'static>], tag: &[u8]) -> bool {
    events.iter().any(|event| match event {
        Event::Start(e) | Event::Empty(e) => local_name(e.name().as_ref()) == tag,
        Event::End(e) => local_name(e.name().as_ref()) == tag,
        _ => false,
    })
}

fn handle_paragraph_start(
    event: &BytesStart<'static>,
    hyperlink_targets: &HashMap<String, String>,
    paragraph_started: &mut bool,
    ppr_depth: &mut usize,
    run_depth: &mut usize,
    run_style_depth: &mut usize,
    in_text: &mut bool,
    math_depth: &mut usize,
    math_text_depth: &mut usize,
    hyperlink_depth: &mut usize,
    current_run_style: &mut RunStyle,
    current_run_property_events: &mut Vec<Event<'static>>,
    current_hyperlink_target: &mut Option<String>,
    current_hyperlink_signature: &mut Option<String>,
    is_heading: &mut bool,
    editable: &mut String,
    editable_presentation: &mut Option<ChunkPresentation>,
    regions: &mut Vec<TextRegion>,
) -> Result<(), String> {
    let name = local_name_owned(event.name().as_ref());
    if !*paragraph_started {
        if name.as_slice() != b"p" {
            return Err("解析 docx 段落失败：未找到段落起始标签。".to_string());
        }
        *paragraph_started = true;
        return Ok(());
    }
    if *math_depth > 0 {
        *math_depth += 1;
        if name.as_slice() == b"t" {
            *math_text_depth += 1;
        }
        return Ok(());
    }
    if *in_text {
        return Err("当前 docx 段落中的文本节点存在嵌套结构。".to_string());
    }
    if *run_depth > 0 {
        return handle_run_start(
            event,
            run_style_depth,
            in_text,
            current_run_style,
            current_run_property_events,
        );
    }
    if *hyperlink_depth > 0 {
        return handle_hyperlink_start(
            event,
            hyperlink_targets,
            hyperlink_depth,
            run_depth,
            math_depth,
            current_run_style,
            current_hyperlink_target,
            current_hyperlink_signature,
            current_run_property_events,
            editable,
            editable_presentation,
            regions,
        );
    }
    if *ppr_depth > 0 {
        if name.as_slice() == b"pStyle" {
            *is_heading = *is_heading
                || attr_value(event, b"val").is_some_and(|value| is_heading_style_id(&value));
        }
        *ppr_depth += 1;
        return Ok(());
    }
    match name.as_slice() {
        b"pPr" => *ppr_depth = 1,
        b"r" => {
            *run_depth = 1;
            *current_run_style = RunStyle::default();
            current_run_property_events.clear();
        }
        b"hyperlink" => {
            *hyperlink_depth = 1;
            *current_hyperlink_target = hyperlink_target(event, hyperlink_targets);
            *current_hyperlink_signature = Some(bytes_start_signature(event));
        }
        b"oMath" | b"oMathPara" => {
            flush_editable_region(regions, editable, editable_presentation, false);
            *math_depth = 1;
        }
        name if is_embedded_object_name(name) => return Err(DOCX_EMBEDDED_OBJECT_ERROR.to_string()),
        _ => {
            return Err(format!(
                "当前 docx 段落内存在不支持的结构 <{}>。",
                tag_name(name.as_slice())
            ))
        }
    }
    Ok(())
}

fn handle_run_start(
    event: &BytesStart<'static>,
    run_style_depth: &mut usize,
    in_text: &mut bool,
    current_run_style: &mut RunStyle,
    current_run_property_events: &mut Vec<Event<'static>>,
) -> Result<(), String> {
    let name = local_name_owned(event.name().as_ref());
    if *run_style_depth > 0 {
        *run_style_depth += 1;
        update_run_style(current_run_style, event);
        current_run_property_events.push(Event::Start(event.clone()));
        return Ok(());
    }
    match name.as_slice() {
        b"t" => *in_text = true,
        b"rPr" => {
            *run_style_depth = 1;
            current_run_property_events.clear();
            current_run_property_events.push(Event::Start(event.clone()));
        }
        name if is_embedded_object_name(name) => return Err(DOCX_EMBEDDED_OBJECT_ERROR.to_string()),
        _ => {
            return Err(format!(
                "当前 docx 运行节点内存在不支持的结构 <{}>。",
                tag_name(name.as_slice())
            ))
        }
    }
    Ok(())
}

fn handle_hyperlink_start(
    event: &BytesStart<'static>,
    _hyperlink_targets: &HashMap<String, String>,
    _hyperlink_depth: &mut usize,
    run_depth: &mut usize,
    _math_depth: &mut usize,
    current_run_style: &mut RunStyle,
    _current_hyperlink_target: &mut Option<String>,
    _current_hyperlink_signature: &mut Option<String>,
    current_run_property_events: &mut Vec<Event<'static>>,
    _editable: &mut String,
    _editable_presentation: &mut Option<ChunkPresentation>,
    _regions: &mut Vec<TextRegion>,
) -> Result<(), String> {
    let name = local_name_owned(event.name().as_ref());
    match name.as_slice() {
        b"r" => {
            *run_depth = 1;
            *current_run_style = RunStyle::default();
            current_run_property_events.clear();
            Ok(())
        }
        b"oMath" | b"oMathPara" => Err(
            "当前不支持超链接内嵌公式的 docx：这类结构无法安全写回，请先在 Word 中调整后再导入。"
                .to_string(),
        ),
        b"hyperlink" => Err("当前 docx 超链接中存在嵌套超链接结构，无法安全导入。".to_string()),
        name if is_embedded_object_name(name) => Err(DOCX_EMBEDDED_OBJECT_ERROR.to_string()),
        _ => Err(format!(
            "当前 docx 超链接内存在不支持的结构 <{}>。",
            tag_name(name.as_slice())
        )),
    }
}

fn handle_paragraph_empty(
    event: &BytesStart<'static>,
    paragraph_started: &mut bool,
    paragraph_finished: &mut bool,
    ppr_depth: &mut usize,
    run_depth: &mut usize,
    run_style_depth: &mut usize,
    hyperlink_depth: &mut usize,
    current_run_style: &mut RunStyle,
    current_run_property_events: &mut Vec<Event<'static>>,
    current_hyperlink_target: &mut Option<String>,
    current_hyperlink_signature: &mut Option<String>,
    is_heading: &mut bool,
    editable: &mut String,
    editable_presentation: &mut Option<ChunkPresentation>,
    regions: &mut Vec<TextRegion>,
) -> Result<(), String> {
    let name = local_name_owned(event.name().as_ref());
    if !*paragraph_started {
        if name.as_slice() != b"p" {
            return Err("解析 docx 段落失败：未找到段落起始标签。".to_string());
        }
        *paragraph_started = true;
        *paragraph_finished = true;
        return Ok(());
    }
    if *run_depth > 0 {
        return handle_run_empty(
            event,
            run_style_depth,
            current_run_style,
            current_run_property_events,
            editable,
            editable_presentation,
            current_hyperlink_target.as_deref(),
            current_hyperlink_signature.as_deref(),
            regions,
        );
    }
    if *hyperlink_depth > 0 {
        return handle_hyperlink_empty(
            event,
            hyperlink_depth,
            current_hyperlink_target,
            current_hyperlink_signature,
        );
    }
    if *ppr_depth > 0 {
        if name.as_slice() == b"pStyle" {
            *is_heading = *is_heading
                || attr_value(event, b"val").is_some_and(|value| is_heading_style_id(&value));
        }
        return Ok(());
    }
    match name.as_slice() {
        b"pPr" | b"r" => {}
        b"oMath" | b"oMathPara" => push_import_region(
            regions,
            String::new(),
            true,
            placeholders::placeholder_presentation("formula"),
        ),
        name if is_embedded_object_name(name) => return Err(DOCX_EMBEDDED_OBJECT_ERROR.to_string()),
        _ => {
            return Err(format!(
                "当前 docx 段落内存在不支持的结构 <{}>。",
                tag_name(name.as_slice())
            ))
        }
    }
    Ok(())
}

fn handle_run_empty(
    event: &BytesStart<'static>,
    run_style_depth: &mut usize,
    current_run_style: &mut RunStyle,
    current_run_property_events: &mut Vec<Event<'static>>,
    editable: &mut String,
    editable_presentation: &mut Option<ChunkPresentation>,
    current_hyperlink_target: Option<&str>,
    current_hyperlink_signature: Option<&str>,
    regions: &mut Vec<TextRegion>,
) -> Result<(), String> {
    let name = local_name_owned(event.name().as_ref());
    if *run_style_depth > 0 {
        update_run_style(current_run_style, event);
        current_run_property_events.push(Event::Empty(event.clone()));
        return Ok(());
    }
    match name.as_slice() {
        b"t" => {}
        b"tab" => append_editable_text(
            regions,
            editable,
            editable_presentation,
            current_editable_presentation(
                current_run_style,
                current_hyperlink_target.map(ToOwned::to_owned),
                current_run_writeback_key(current_run_property_events, current_hyperlink_signature),
            ),
            "\t",
        ),
        b"br" if is_page_break(event) => {
            if current_hyperlink_target.is_some() {
                return Err(DOCX_HYPERLINK_PAGE_BREAK_ERROR.to_string());
            }
            flush_editable_region(regions, editable, editable_presentation, false);
            push_import_region(
                regions,
                DOCX_PAGE_BREAK_PLACEHOLDER.to_string(),
                true,
                placeholders::placeholder_presentation("page-break"),
            );
        }
        b"br" | b"cr" => append_editable_text(
            regions,
            editable,
            editable_presentation,
            current_editable_presentation(
                current_run_style,
                current_hyperlink_target.map(ToOwned::to_owned),
                current_run_writeback_key(current_run_property_events, current_hyperlink_signature),
            ),
            "\n",
        ),
        b"rPr" => {
            current_run_property_events.clear();
            current_run_property_events.push(Event::Empty(event.clone()));
        }
        name if is_embedded_object_name(name) => return Err(DOCX_EMBEDDED_OBJECT_ERROR.to_string()),
        _ => {
            return Err(format!(
                "当前 docx 运行节点内存在不支持的结构 <{}>。",
                tag_name(name.as_slice())
            ))
        }
    }
    Ok(())
}

fn handle_hyperlink_empty(
    event: &BytesStart<'static>,
    _hyperlink_depth: &mut usize,
    _current_hyperlink_target: &mut Option<String>,
    _current_hyperlink_signature: &mut Option<String>,
) -> Result<(), String> {
    let name = local_name_owned(event.name().as_ref());
    match name.as_slice() {
        b"r" => Ok(()),
        name if is_embedded_object_name(name) => Err(DOCX_EMBEDDED_OBJECT_ERROR.to_string()),
        _ => Err(format!(
            "当前 docx 超链接内存在不支持的结构 <{}>。",
            tag_name(name.as_slice())
        )),
    }
}

fn update_run_style(current_run_style: &mut RunStyle, event: &BytesStart<'_>) {
    match local_name(event.name().as_ref()) {
        b"b" => current_run_style.bold = toggle_attr_enabled(event),
        b"i" => current_run_style.italic = toggle_attr_enabled(event),
        b"u" => current_run_style.underline = underline_enabled(event),
        _ => {}
    }
}

fn handle_paragraph_end(
    event: &BytesEnd<'static>,
    paragraph_finished: &mut bool,
    ppr_depth: &mut usize,
    run_depth: &mut usize,
    run_style_depth: &mut usize,
    in_text: &mut bool,
    math_depth: &mut usize,
    math_text_depth: &mut usize,
    formula: &mut String,
    hyperlink_depth: &mut usize,
    current_hyperlink_target: &mut Option<String>,
    current_hyperlink_signature: &mut Option<String>,
    current_run_property_events: &mut Vec<Event<'static>>,
    regions: &mut Vec<TextRegion>,
) -> Result<(), String> {
    let name = local_name_owned(event.name().as_ref());
    if *math_depth > 0 {
        if name.as_slice() == b"t" && *math_text_depth > 0 {
            *math_text_depth -= 1;
        }
        *math_depth -= 1;
        if *math_depth == 0 {
            flush_locked_region(regions, formula, "formula");
        }
        return Ok(());
    }
    if *in_text {
        if name.as_slice() != b"t" {
            return Err("解析 docx 段落失败：文本节点闭合异常。".to_string());
        }
        *in_text = false;
        return Ok(());
    }
    if *run_style_depth > 0 {
        current_run_property_events.push(Event::End(event.clone()));
        *run_style_depth -= 1;
        return Ok(());
    }
    if *run_depth > 0 {
        if name.as_slice() == b"r" {
            *run_depth -= 1;
        }
        return Ok(());
    }
    if *hyperlink_depth > 0 {
        if name.as_slice() == b"hyperlink" {
            *hyperlink_depth -= 1;
            if *hyperlink_depth == 0 {
                *current_hyperlink_target = None;
                *current_hyperlink_signature = None;
            }
        }
        return Ok(());
    }
    if *ppr_depth > 0 {
        *ppr_depth -= 1;
        return Ok(());
    }
    if name.as_slice() == b"p" {
        *paragraph_finished = true;
    }
    Ok(())
}

fn append_paragraph_text(
    text: &str,
    in_text: bool,
    in_math_text: bool,
    editable: &mut String,
    editable_presentation: &mut Option<ChunkPresentation>,
    current_presentation: Option<ChunkPresentation>,
    formula: &mut String,
    regions: &mut Vec<TextRegion>,
) -> Result<(), String> {
    if in_math_text {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            formula.push_str(trimmed);
        }
        return Ok(());
    }
    if in_text {
        append_editable_text(
            regions,
            editable,
            editable_presentation,
            current_presentation,
            text,
        );
        return Ok(());
    }
    if text.trim().is_empty() {
        return Ok(());
    }
    Err("当前 docx 段落中存在游离文本节点，无法安全导入。".to_string())
}

fn flatten_import_blocks(blocks: Vec<Vec<TextRegion>>) -> Vec<TextRegion> {
    let total = blocks.len();
    let mut out: Vec<TextRegion> = Vec::new();
    for (index, mut block) in blocks.into_iter().enumerate() {
        if index + 1 < total {
            append_block_separator(&mut block);
        }
        out.extend(block);
    }
    out
}

fn append_block_separator(block: &mut Vec<TextRegion>) {
    if let Some(last) = block.last_mut() {
        last.body.push_str(DOCX_BLOCK_SEPARATOR);
        return;
    }
    block.push(TextRegion {
        body: DOCX_BLOCK_SEPARATOR.to_string(),
        skip_rewrite: false,
        presentation: None,
    });
}

fn flush_editable_region(
    regions: &mut Vec<TextRegion>,
    buffer: &mut String,
    presentation: &mut Option<ChunkPresentation>,
    skip_rewrite: bool,
) {
    if buffer.is_empty() {
        return;
    }
    push_import_region(
        regions,
        std::mem::take(buffer),
        skip_rewrite,
        presentation.take(),
    );
}

fn flush_locked_region(regions: &mut Vec<TextRegion>, buffer: &mut String, kind: &str) {
    if buffer.is_empty() {
        return;
    }
    push_import_region(
        regions,
        std::mem::take(buffer),
        true,
        placeholders::placeholder_presentation(kind),
    );
}

fn push_import_region(
    regions: &mut Vec<TextRegion>,
    body: String,
    skip_rewrite: bool,
    presentation: Option<ChunkPresentation>,
) {
    if body.is_empty() && !regions.is_empty() {
        return;
    }
    if let Some(last) = regions.last_mut() {
        if last.skip_rewrite == skip_rewrite && last.presentation == presentation {
            last.body.push_str(&body);
            return;
        }
    }
    regions.push(TextRegion {
        body,
        skip_rewrite,
        presentation,
    });
}

fn empty_region(skip_rewrite: bool) -> TextRegion {
    TextRegion {
        body: String::new(),
        skip_rewrite,
        presentation: None,
    }
}

fn append_editable_text(
    regions: &mut Vec<TextRegion>,
    editable: &mut String,
    editable_presentation: &mut Option<ChunkPresentation>,
    current_presentation: Option<ChunkPresentation>,
    text: &str,
) {
    if text.is_empty() {
        return;
    }
    if editable.is_empty() {
        *editable_presentation = current_presentation;
        editable.push_str(text);
        return;
    }
    if *editable_presentation == current_presentation {
        editable.push_str(text);
        return;
    }
    flush_editable_region(regions, editable, editable_presentation, false);
    *editable_presentation = current_presentation;
    editable.push_str(text);
}

fn append_start_signature(signature: &mut String, prefix: char, event: &BytesStart<'_>) {
    let event_name_binding = event.name();
    let event_name = event_name_binding.as_ref();
    signature.push(prefix);
    signature.push_str(std::str::from_utf8(event_name).unwrap_or("?"));
    let event_name = local_name(event_name);
    for attr in event.attributes().flatten() {
        if should_ignore_signature_attr(event_name, attr.key.as_ref()) {
            continue;
        }
        signature.push('|');
        signature.push_str(std::str::from_utf8(attr.key.as_ref()).unwrap_or("?"));
        signature.push('=');
        let value = attr
            .unescape_value()
            .map(|value| value.into_owned())
            .unwrap_or_else(|_| String::from_utf8_lossy(attr.value.as_ref()).into_owned());
        signature.push_str(&value);
    }
    signature.push(';');
}

fn should_ignore_signature_attr(event_name: &[u8], attr_key: &[u8]) -> bool {
    event_name == b"rFonts" && local_name(attr_key) == b"hint"
}

fn append_end_signature(signature: &mut String, event: &BytesEnd<'_>) {
    signature.push('E');
    signature.push_str(std::str::from_utf8(event.name().as_ref()).unwrap_or("?"));
    signature.push(';');
}

fn append_text_signature(signature: &mut String, prefix: char, text: &str) {
    signature.push(prefix);
    signature.push_str(text);
    signature.push(';');
}

fn append_non_whitespace_signature(signature: &mut String, prefix: char, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    append_text_signature(signature, prefix, text);
}

fn bytes_start_signature(event: &BytesStart<'_>) -> String {
    let mut signature = String::new();
    append_start_signature(&mut signature, 'S', event);
    signature
}

fn events_signature(events: &[Event<'static>]) -> String {
    let mut signature = String::new();
    for event in events {
        match event {
            Event::Start(e) => append_start_signature(&mut signature, 'S', e),
            Event::Empty(e) => append_start_signature(&mut signature, 'X', e),
            Event::End(e) => append_end_signature(&mut signature, e),
            Event::Text(e) => append_non_whitespace_signature(
                &mut signature,
                'T',
                &String::from_utf8_lossy(e.as_ref()),
            ),
            Event::CData(e) => append_non_whitespace_signature(
                &mut signature,
                'C',
                &String::from_utf8_lossy(e.as_ref()),
            ),
            Event::Comment(e) => append_non_whitespace_signature(
                &mut signature,
                'M',
                &String::from_utf8_lossy(e.as_ref()),
            ),
            Event::Decl(e) => append_non_whitespace_signature(
                &mut signature,
                'D',
                &String::from_utf8_lossy(e.as_ref()),
            ),
            Event::PI(e) => append_non_whitespace_signature(
                &mut signature,
                'P',
                &String::from_utf8_lossy(e.content()),
            ),
            Event::DocType(e) => append_non_whitespace_signature(
                &mut signature,
                'O',
                &String::from_utf8_lossy(e.as_ref()),
            ),
            Event::GeneralRef(e) => append_non_whitespace_signature(
                &mut signature,
                'G',
                &String::from_utf8_lossy(e.as_ref()),
            ),
            Event::Eof => {}
        }
    }
    signature
}

fn run_property_signature(events: &[Event<'static>]) -> String {
    events_signature(&normalize_run_property_events(events))
}

fn normalize_run_property_events(events: &[Event<'static>]) -> Vec<Event<'static>> {
    let filtered = events
        .iter()
        .filter(|event| !should_drop_run_property_event(event))
        .cloned()
        .collect::<Vec<_>>();

    strip_empty_run_property_wrapper(filtered)
}

fn should_drop_run_property_event(event: &Event<'static>) -> bool {
    match event {
        Event::Empty(e) => should_drop_empty_run_property_event(e),
        _ => false,
    }
}

fn should_drop_empty_run_property_event(event: &BytesStart<'_>) -> bool {
    let name_binding = event.name();
    let name = local_name(name_binding.as_ref());
    matches!(name, b"rPr" | b"rFonts") && !has_meaningful_signature_attr(name, event)
}

fn has_meaningful_signature_attr(event_name: &[u8], event: &BytesStart<'_>) -> bool {
    event
        .attributes()
        .flatten()
        .any(|attr| !should_ignore_signature_attr(event_name, attr.key.as_ref()))
}

fn strip_empty_run_property_wrapper(events: Vec<Event<'static>>) -> Vec<Event<'static>> {
    if matches!(
        events.as_slice(),
        [Event::Empty(e)] if local_name(e.name().as_ref()) == b"rPr"
    ) {
        return Vec::new();
    }

    if matches!(
        events.as_slice(),
        [Event::Start(start), Event::End(end)]
            if local_name(start.name().as_ref()) == b"rPr"
                && local_name(end.name().as_ref()) == b"rPr"
    ) {
        return Vec::new();
    }

    events
}

fn build_editable_writeback_key(
    run_property_signature: &str,
    hyperlink_signature: Option<&str>,
) -> Option<String> {
    if run_property_signature.is_empty() && hyperlink_signature.is_none() {
        return None;
    }
    let mut key = String::new();
    if !run_property_signature.is_empty() {
        key.push_str("r:");
        key.push_str(run_property_signature);
    }
    if let Some(hyperlink_signature) = hyperlink_signature {
        if !key.is_empty() {
            key.push('|');
        }
        key.push_str("h:");
        key.push_str(hyperlink_signature);
    }
    Some(key)
}

fn current_run_writeback_key(
    run_property_events: &[Event<'static>],
    hyperlink_signature: Option<&str>,
) -> Option<String> {
    build_editable_writeback_key(
        &run_property_signature(run_property_events),
        hyperlink_signature,
    )
}

fn current_editable_presentation(
    run_style: &RunStyle,
    href: Option<String>,
    writeback_key: Option<String>,
) -> Option<ChunkPresentation> {
    if !run_style.bold
        && !run_style.italic
        && !run_style.underline
        && href.is_none()
        && writeback_key.is_none()
    {
        return None;
    }
    Some(ChunkPresentation {
        bold: run_style.bold,
        italic: run_style.italic,
        underline: run_style.underline,
        href,
        protect_kind: None,
        writeback_key,
    })
}

fn mark_all_regions_locked(regions: &mut [TextRegion]) {
    for region in regions {
        region.skip_rewrite = true;
    }
}

fn trim_paragraph_bom(paragraphs: &mut [DocxParagraph]) {
    if let Some(first) = paragraphs.first_mut() {
        first.text = first.text.trim_start_matches('\u{feff}').to_string();
    }
    if let Some(last) = paragraphs.last_mut() {
        last.text = last.text.trim_end_matches('\u{feff}').to_string();
    }
}

fn trim_region_bom(blocks: &mut [Vec<TextRegion>]) {
    if let Some(first) = blocks.first_mut().and_then(|block| block.first_mut()) {
        first.body = first.body.trim_start_matches('\u{feff}').to_string();
    }
    if let Some(last) = blocks.last_mut().and_then(|block| block.last_mut()) {
        last.body = last.body.trim_end_matches('\u{feff}').to_string();
    }
}

fn is_page_break(event: &BytesStart<'_>) -> bool {
    attr_value(event, b"type").as_deref() == Some("page")
}

fn is_embedded_object_name(name: &[u8]) -> bool {
    matches!(name, b"object" | b"OLEObject" | b"chart" | b"relIds")
}

fn extract_writeback_paragraph_templates(
    xml: &str,
    hyperlink_targets: &HashMap<String, String>,
) -> Result<Vec<WritebackBlockTemplate>, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);

    let mut buf = Vec::new();
    let mut body_depth = 0usize;
    let mut block_depth = 0usize;
    let mut block_name: Option<Vec<u8>> = None;
    let mut block_events: Vec<Event<'static>> = Vec::new();
    let mut blocks = Vec::new();

    loop {
        let event = match reader.read_event_into(&mut buf) {
            Ok(event) => event.into_owned(),
            Err(error) => return Err(format!("解析 document.xml 失败：{error}")),
        };

        match event {
            Event::Start(e) => {
                let name = local_name_owned(e.name().as_ref());
                if block_depth > 0 {
                    block_depth += 1;
                    block_events.push(Event::Start(e));
                } else if name.as_slice() == b"body" {
                    body_depth = 1;
                } else if body_depth > 0 {
                    if body_depth == 1 {
                        match name.as_slice() {
                            b"p" | b"tbl" | b"sdt" => {
                                block_depth = 1;
                                block_name = Some(name.clone());
                                block_events.clear();
                                block_events.push(Event::Start(e));
                            }
                            b"sectPr" => {
                                block_depth = 1;
                                block_name = Some(name.clone());
                                block_events.clear();
                                block_events.push(Event::Start(e));
                            }
                            _ => {
                                return Err(format!(
                                    "当前仅支持文章语义相关的 docx：检测到不支持的正文结构 <{}>。",
                                    tag_name(name.as_slice())
                                ))
                            }
                        }
                    } else {
                        body_depth += 1;
                    }
                }
            }
            Event::Empty(e) => {
                let name = local_name_owned(e.name().as_ref());
                if block_depth > 0 {
                    block_events.push(Event::Empty(e));
                } else if body_depth > 0 && body_depth == 1 {
                    match name.as_slice() {
                        b"p" => blocks.push(WritebackBlockTemplate::Paragraph(
                            parse_writeback_paragraph_template(
                                &[Event::Empty(e)],
                                hyperlink_targets,
                            )?,
                        )),
                        b"tbl" => blocks.push(parse_table_placeholder_block(&[Event::Empty(e)])?),
                        b"sdt" => blocks.push(parse_toc_placeholder_block(&[Event::Empty(e)])?),
                        b"sectPr" => {
                            blocks.push(parse_section_break_placeholder_block(&[Event::Empty(e)])?)
                        }
                        _ => {
                            return Err(format!(
                                "当前仅支持文章语义相关的 docx：检测到不支持的正文结构 <{}>。",
                                tag_name(name.as_slice())
                            ))
                        }
                    }
                }
            }
            Event::End(e) => {
                let name = local_name_owned(e.name().as_ref());
                if block_depth > 0 {
                    block_events.push(Event::End(e));
                    block_depth -= 1;
                    if block_depth == 0 {
                        let kind = block_name.take().unwrap_or_default();
                        let block = match kind.as_slice() {
                            b"p" => WritebackBlockTemplate::Paragraph(
                                parse_writeback_paragraph_template(
                                    &block_events,
                                    hyperlink_targets,
                                )?,
                            ),
                            b"tbl" => parse_table_placeholder_block(&block_events)?,
                            b"sdt" => parse_toc_placeholder_block(&block_events)?,
                            b"sectPr" => parse_section_break_placeholder_block(&block_events)?,
                            _ => return Err("解析 docx 写回模板失败：未知正文块类型。".to_string()),
                        };
                        blocks.push(block);
                        block_events.clear();
                    }
                } else if body_depth > 0 {
                    if name.as_slice() == b"body" && body_depth == 1 {
                        body_depth = 0;
                    } else {
                        body_depth -= 1;
                    }
                }
            }
            Event::Text(_)
            | Event::CData(_)
            | Event::Comment(_)
            | Event::Decl(_)
            | Event::PI(_)
            | Event::DocType(_)
            | Event::GeneralRef(_) => {
                if block_depth > 0 {
                    block_events.push(event);
                }
            }
            Event::Eof => break,
        }

        buf.clear();
    }

    trim_writeback_block_bom(&mut blocks);
    Ok(blocks)
}

fn parse_table_placeholder_block(
    events: &[Event<'static>],
) -> Result<WritebackBlockTemplate, String> {
    parse_locked_block(events, placeholders::DOCX_TABLE_PLACEHOLDER, "table")
}

fn parse_section_break_placeholder_block(
    events: &[Event<'static>],
) -> Result<WritebackBlockTemplate, String> {
    parse_locked_block(
        events,
        placeholders::DOCX_SECTION_BREAK_PLACEHOLDER,
        "section-break",
    )
}

fn parse_toc_placeholder_block(
    events: &[Event<'static>],
) -> Result<WritebackBlockTemplate, String> {
    parse_locked_block(events, placeholders::DOCX_TOC_PLACEHOLDER, "toc")
}

fn parse_locked_block(
    events: &[Event<'static>],
    text: &str,
    protect_kind: &str,
) -> Result<WritebackBlockTemplate, String> {
    if events.is_empty() {
        return Err("解析 docx 锁定块失败：事件为空。".to_string());
    }
    Ok(placeholders::raw_locked_block(text, protect_kind, events))
}

fn next_index_after_ignorable_writeback_event(
    events: &[Event<'static>],
    start_index: usize,
) -> Result<Option<usize>, String> {
    let Some(event) = events.get(start_index) else {
        return Ok(None);
    };
    match event {
        Event::Start(e) if is_ignorable_paragraph_name(local_name(e.name().as_ref())) => {
            Ok(Some(skip_subtree_events(events, start_index)?))
        }
        Event::Empty(e) if is_ignorable_paragraph_name(local_name(e.name().as_ref())) => {
            Ok(Some(start_index + 1))
        }
        _ => Ok(None),
    }
}

fn trim_writeback_block_bom(blocks: &mut [WritebackBlockTemplate]) {
    if let Some(first) = blocks.first_mut() {
        match first {
            WritebackBlockTemplate::Paragraph(paragraph) => {
                if let Some(region) = paragraph.regions.first_mut() {
                    trim_region_start_bom(region);
                }
            }
            WritebackBlockTemplate::Locked(region) => {
                region.text = region.text.trim_start_matches('\u{feff}').to_string();
            }
        }
    }
    if let Some(last) = blocks.last_mut() {
        match last {
            WritebackBlockTemplate::Paragraph(paragraph) => {
                if let Some(region) = paragraph.regions.last_mut() {
                    trim_region_end_bom(region);
                }
            }
            WritebackBlockTemplate::Locked(region) => {
                region.text = region.text.trim_end_matches('\u{feff}').to_string();
            }
        }
    }
}

fn trim_region_start_bom(region: &mut WritebackRegionTemplate) {
    match region {
        WritebackRegionTemplate::Editable(editable) => {
            editable.text = editable.text.trim_start_matches('\u{feff}').to_string();
        }
        WritebackRegionTemplate::Locked(locked) => {
            locked.text = locked.text.trim_start_matches('\u{feff}').to_string();
        }
    }
}

fn trim_region_end_bom(region: &mut WritebackRegionTemplate) {
    match region {
        WritebackRegionTemplate::Editable(editable) => {
            editable.text = editable.text.trim_end_matches('\u{feff}').to_string();
        }
        WritebackRegionTemplate::Locked(locked) => {
            locked.text = locked.text.trim_end_matches('\u{feff}').to_string();
        }
    }
}

fn build_writeback_source_text(blocks: &[WritebackBlockTemplate]) -> String {
    blocks
        .iter()
        .map(WritebackBlockTemplate::text)
        .collect::<Vec<_>>()
        .join(DOCX_BLOCK_SEPARATOR)
        .trim_matches('\u{feff}')
        .to_string()
}

#[cfg(test)]
fn flatten_writeback_blocks_for_test(blocks: &[WritebackBlockTemplate]) -> Vec<TextRegion> {
    let mut regions = Vec::new();
    for (block_index, block) in blocks.iter().enumerate() {
        let append_block_separator = block_index + 1 < blocks.len();
        match block {
            WritebackBlockTemplate::Paragraph(paragraph) => {
                if paragraph.regions.is_empty() {
                    regions.push(TextRegion {
                        body: if append_block_separator {
                            DOCX_BLOCK_SEPARATOR.to_string()
                        } else {
                            String::new()
                        },
                        skip_rewrite: paragraph.is_heading,
                        presentation: None,
                    });
                    continue;
                }
                for (region_index, region) in paragraph.regions.iter().enumerate() {
                    let mut body = region.text().to_string();
                    if append_block_separator && region_index + 1 == paragraph.regions.len() {
                        body.push_str(DOCX_BLOCK_SEPARATOR);
                    }
                    let mut presentation = region.presentation().cloned();
                    if paragraph.is_heading && !region.skip_rewrite() {
                        if let Some(presentation) = presentation.as_mut() {
                            presentation.writeback_key = region
                                .presentation()
                                .and_then(|item| item.writeback_key.clone());
                        }
                    }
                    regions.push(TextRegion {
                        body,
                        skip_rewrite: paragraph.is_heading || region.skip_rewrite(),
                        presentation,
                    });
                }
            }
            WritebackBlockTemplate::Locked(region) => {
                let mut body = region.text.clone();
                if append_block_separator {
                    body.push_str(DOCX_BLOCK_SEPARATOR);
                }
                regions.push(TextRegion {
                    body,
                    skip_rewrite: true,
                    presentation: region.presentation.clone(),
                });
            }
        }
    }
    regions
}

fn parse_writeback_paragraph_template(
    events: &[Event<'static>],
    hyperlink_targets: &HashMap<String, String>,
) -> Result<WritebackParagraphTemplate, String> {
    let (paragraph_start, paragraph_end) = paragraph_bounds(events)?;
    if events.len() == 1 {
        return Ok(WritebackParagraphTemplate {
            paragraph_start,
            paragraph_end,
            is_heading: false,
            paragraph_property_events: Vec::new(),
            regions: Vec::new(),
        });
    }

    let mut regions = Vec::new();
    let paragraph_property_events = collect_paragraph_property_events(events);
    let is_heading = paragraph_properties_indicate_heading(&paragraph_property_events);
    let mut index = 1usize;
    let limit = events.len().saturating_sub(1);

    while index < limit {
        if let Some(next_index) = next_index_after_ignorable_writeback_event(events, index)? {
            index = next_index;
            continue;
        }
        match &events[index] {
            Event::Start(e) | Event::Empty(e) => {
                let name = local_name_owned(e.name().as_ref());
                if name.as_slice() == b"pPr" {
                    index = skip_subtree_events(events, index)?;
                    continue;
                }
                let (child_events, next_index) = capture_subtree_events(events, index)?;
                match name.as_slice() {
                    b"r" => regions.extend(parse_writeback_run_regions(&child_events, None, None)?),
                    b"hyperlink" => regions.extend(parse_writeback_hyperlink_regions(
                        &child_events,
                        hyperlink_targets,
                    )?),
                    b"oMath" | b"oMathPara" => {
                        regions.push(parse_writeback_formula_region(&child_events)?)
                    }
                    name if is_locked_inline_object_name(name) => {
                        let (text, kind) = classify_locked_object_placeholder(&child_events)?;
                        regions.push(placeholders::raw_locked_region(text, kind, &child_events))
                    }
                    name if is_embedded_object_name(name) => {
                        return Err(DOCX_EMBEDDED_OBJECT_ERROR.to_string())
                    }
                    _ => {
                        return Err(format!(
                            "当前仅支持文章语义相关的 docx：段落内存在不支持的结构 <{}>。",
                            tag_name(name.as_slice())
                        ))
                    }
                }
                index = next_index;
            }
            Event::Text(e) => {
                let decoded = e
                    .decode()
                    .map_err(|error| format!("解析 document.xml 文本失败：{error}"))?;
                if !decoded.trim().is_empty() {
                    return Err("当前 docx 段落中存在游离文本节点，无法安全写回。".to_string());
                }
                index += 1;
            }
            Event::CData(e) => {
                let decoded = e
                    .decode()
                    .map_err(|error| format!("解析 document.xml CDATA 失败：{error}"))?;
                if !decoded.trim().is_empty() {
                    return Err("当前 docx 段落中存在游离 CDATA，无法安全写回。".to_string());
                }
                index += 1;
            }
            Event::Comment(_)
            | Event::Decl(_)
            | Event::PI(_)
            | Event::DocType(_)
            | Event::GeneralRef(_) => index += 1,
            Event::End(_) | Event::Eof => index += 1,
        }
    }

    Ok(WritebackParagraphTemplate {
        paragraph_start,
        paragraph_end,
        is_heading,
        paragraph_property_events,
        regions: merge_adjacent_writeback_regions(regions),
    })
}

fn skip_subtree_events(events: &[Event<'static>], start_index: usize) -> Result<usize, String> {
    let (_, next_index) = capture_subtree_events(events, start_index)?;
    Ok(next_index)
}

fn capture_subtree_events(
    events: &[Event<'static>],
    start_index: usize,
) -> Result<(Vec<Event<'static>>, usize), String> {
    let Some(first) = events.get(start_index) else {
        return Err("解析 docx 写回模板失败：子树起点越界。".to_string());
    };
    match first {
        Event::Empty(e) => Ok((vec![Event::Empty(e.clone())], start_index + 1)),
        Event::Start(e) => {
            let mut depth = 1usize;
            let mut out = vec![Event::Start(e.clone())];
            let mut index = start_index + 1;
            while index < events.len() {
                let event = events[index].clone();
                match &event {
                    Event::Start(_) => depth += 1,
                    Event::End(_) => {
                        depth -= 1;
                        out.push(event);
                        index += 1;
                        if depth == 0 {
                            return Ok((out, index));
                        }
                        continue;
                    }
                    _ => {}
                }
                out.push(event);
                index += 1;
            }
            Err("解析 docx 写回模板失败：子树未正常闭合。".to_string())
        }
        _ => Err("解析 docx 写回模板失败：非法子树起点。".to_string()),
    }
}

fn parse_writeback_hyperlink_regions(
    events: &[Event<'static>],
    hyperlink_targets: &HashMap<String, String>,
) -> Result<Vec<WritebackRegionTemplate>, String> {
    let Some(Event::Start(start) | Event::Empty(start)) = events.first() else {
        return Err("解析 docx 超链接写回模板失败：未找到超链接起始标签。".to_string());
    };
    let hyperlink_start = start.clone();
    let hyperlink_href = hyperlink_target(&hyperlink_start, hyperlink_targets);
    let mut regions = Vec::new();
    let mut index = 1usize;
    let limit = events.len().saturating_sub(1);

    while index < limit {
        if let Some(next_index) = next_index_after_ignorable_writeback_event(events, index)? {
            index = next_index;
            continue;
        }
        match &events[index] {
            Event::Start(e) | Event::Empty(e) => {
                let name = local_name_owned(e.name().as_ref());
                let (child_events, next_index) = capture_subtree_events(events, index)?;
                match name.as_slice() {
                    b"r" => regions.extend(parse_writeback_run_regions(
                        &child_events,
                        Some(&hyperlink_start),
                        hyperlink_href.clone(),
                    )?),
                    name if is_locked_inline_object_name(name) => {
                        let (text, kind) = classify_locked_object_placeholder(&child_events)?;
                        regions.push(placeholders::raw_locked_region(text, kind, &child_events))
                    }
                    name if is_embedded_object_name(name) => {
                        return Err(DOCX_EMBEDDED_OBJECT_ERROR.to_string())
                    }
                    _ => {
                        return Err(format!(
                            "当前仅支持正文超链接文本写回：超链接内存在不支持的结构 <{}>。",
                            tag_name(name.as_slice())
                        ))
                    }
                }
                index = next_index;
            }
            Event::Text(e) => {
                let decoded = e
                    .decode()
                    .map_err(|error| format!("解析 document.xml 文本失败：{error}"))?;
                if !decoded.trim().is_empty() {
                    return Err("当前 docx 超链接中存在游离文本节点，无法安全写回。".to_string());
                }
                index += 1;
            }
            Event::CData(e) => {
                let decoded = e
                    .decode()
                    .map_err(|error| format!("解析 document.xml CDATA 失败：{error}"))?;
                if !decoded.trim().is_empty() {
                    return Err("当前 docx 超链接中存在游离 CDATA，无法安全写回。".to_string());
                }
                index += 1;
            }
            Event::Comment(_)
            | Event::Decl(_)
            | Event::PI(_)
            | Event::DocType(_)
            | Event::GeneralRef(_) => index += 1,
            Event::End(_) | Event::Eof => index += 1,
        }
    }

    Ok(regions)
}

fn merge_adjacent_writeback_regions(
    regions: Vec<WritebackRegionTemplate>,
) -> Vec<WritebackRegionTemplate> {
    let mut merged = Vec::new();
    for region in regions {
        if let Some(last) = merged.last_mut() {
            if merge_writeback_region(last, &region) {
                continue;
            }
        }
        merged.push(region);
    }
    merged
}

fn merge_writeback_region(
    current: &mut WritebackRegionTemplate,
    next: &WritebackRegionTemplate,
) -> bool {
    match (current, next) {
        (WritebackRegionTemplate::Editable(current), WritebackRegionTemplate::Editable(next)) => {
            if !can_merge_editable_writeback_regions(current, next) {
                return false;
            }
            current.text.push_str(&next.text);
            true
        }
        (WritebackRegionTemplate::Locked(current), WritebackRegionTemplate::Locked(next)) => {
            merge_locked_writeback_regions(current, next)
        }
        _ => false,
    }
}

fn merge_locked_writeback_regions(
    current: &mut LockedRegionTemplate,
    next: &LockedRegionTemplate,
) -> bool {
    if current.presentation != next.presentation {
        return false;
    }
    current.text.push_str(&next.text);
    current.render = merge_locked_region_render(current.render.clone(), next.render.clone());
    true
}

fn merge_locked_region_render(
    current: LockedRegionRender,
    next: LockedRegionRender,
) -> LockedRegionRender {
    let mut items = Vec::new();
    extend_locked_region_render(&mut items, current);
    extend_locked_region_render(&mut items, next);
    LockedRegionRender::Sequence(items)
}

fn extend_locked_region_render(items: &mut Vec<LockedRegionRender>, render: LockedRegionRender) {
    match render {
        LockedRegionRender::Sequence(nested) => items.extend(nested),
        other => items.push(other),
    }
}

fn can_merge_editable_writeback_regions(
    current: &EditableRegionTemplate,
    next: &EditableRegionTemplate,
) -> bool {
    if current.presentation != next.presentation {
        return false;
    }
    match (&current.render, &next.render) {
        (
            EditableRegionRender::Run {
                run_property_events: current_props,
            },
            EditableRegionRender::Run {
                run_property_events: next_props,
            },
        ) => run_property_signature(current_props) == run_property_signature(next_props),
        (
            EditableRegionRender::Hyperlink {
                hyperlink_start: current_start,
                run_property_events: current_props,
            },
            EditableRegionRender::Hyperlink {
                hyperlink_start: next_start,
                run_property_events: next_props,
            },
        ) => {
            current_start == next_start
                && run_property_signature(current_props) == run_property_signature(next_props)
        }
        _ => false,
    }
}

fn parse_writeback_run_regions(
    events: &[Event<'static>],
    hyperlink_start: Option<&BytesStart<'static>>,
    hyperlink_href: Option<String>,
) -> Result<Vec<WritebackRegionTemplate>, String> {
    let run_property_events = collect_run_property_events(events);
    let hyperlink_signature = hyperlink_start.map(bytes_start_signature);
    let presentation = build_run_presentation(
        &run_property_events,
        hyperlink_href.clone(),
        hyperlink_signature.as_deref(),
    );
    let mut regions = Vec::new();
    let mut buffer = String::new();
    let mut in_text = false;
    let mut rpr_depth = 0usize;
    let mut index = 1usize;
    let limit = events.len().saturating_sub(1);

    while index < limit {
        if let Some(next_index) = next_index_after_ignorable_writeback_event(events, index)? {
            index = next_index;
            continue;
        }
        match &events[index] {
            Event::Start(e) => {
                let name = local_name_owned(e.name().as_ref());
                if in_text {
                    return Err("当前 docx 运行节点中的文本节点存在嵌套结构。".to_string());
                }
                if rpr_depth > 0 {
                    rpr_depth += 1;
                    index += 1;
                    continue;
                }
                if is_locked_inline_object_name(name.as_slice()) {
                    let (child_events, next_index) = capture_subtree_events(events, index)?;
                    flush_writeback_editable_region(
                        &mut regions,
                        &mut buffer,
                        presentation.clone(),
                        hyperlink_start.cloned(),
                        &run_property_events,
                    );
                    let (text, kind) = classify_locked_object_placeholder(&child_events)?;
                    regions.push(placeholders::raw_locked_region(text, kind, &child_events));
                    index = next_index;
                    continue;
                }
                match name.as_slice() {
                    b"rPr" => {
                        rpr_depth = 1;
                        index += 1;
                    }
                    b"t" => {
                        in_text = true;
                        index += 1;
                    }
                    name if is_embedded_object_name(name) => {
                        return Err(DOCX_EMBEDDED_OBJECT_ERROR.to_string())
                    }
                    _ => {
                        return Err(format!(
                            "当前仅支持正文行内写回：运行节点内存在不支持的结构 <{}>。",
                            tag_name(name.as_slice())
                        ))
                    }
                }
            }
            Event::Empty(e) => {
                let name = local_name_owned(e.name().as_ref());
                if rpr_depth > 0 {
                    index += 1;
                    continue;
                }
                if is_locked_inline_object_name(name.as_slice()) {
                    flush_writeback_editable_region(
                        &mut regions,
                        &mut buffer,
                        presentation.clone(),
                        hyperlink_start.cloned(),
                        &run_property_events,
                    );
                    let empty_events = [Event::Empty(e.clone())];
                    let (text, kind) = classify_locked_object_placeholder(&empty_events)?;
                    regions.push(placeholders::raw_locked_region(text, kind, &empty_events));
                    index += 1;
                    continue;
                }
                match name.as_slice() {
                    b"t" | b"rPr" => {}
                    b"tab" => buffer.push('\t'),
                    b"br" if is_page_break(e) => {
                        flush_writeback_editable_region(
                            &mut regions,
                            &mut buffer,
                            presentation.clone(),
                            hyperlink_start.cloned(),
                            &run_property_events,
                        );
                        if hyperlink_start.is_some() {
                            return Err(DOCX_HYPERLINK_PAGE_BREAK_ERROR.to_string());
                        }
                        regions.push(WritebackRegionTemplate::Locked(LockedRegionTemplate {
                            text: DOCX_PAGE_BREAK_PLACEHOLDER.to_string(),
                            presentation: placeholders::placeholder_presentation("page-break"),
                            render: LockedRegionRender::PageBreak,
                        }));
                    }
                    b"br" | b"cr" => buffer.push('\n'),
                    name if is_embedded_object_name(name) => {
                        return Err(DOCX_EMBEDDED_OBJECT_ERROR.to_string())
                    }
                    _ => {
                        return Err(format!(
                            "当前仅支持正文行内写回：运行节点内存在不支持的结构 <{}>。",
                            tag_name(name.as_slice())
                        ))
                    }
                }
                index += 1;
            }
            Event::End(e) => {
                let name = local_name_owned(e.name().as_ref());
                if in_text {
                    if name.as_slice() != b"t" {
                        return Err("解析 docx 运行节点失败：文本节点闭合异常。".to_string());
                    }
                    in_text = false;
                    index += 1;
                    continue;
                }
                if rpr_depth > 0 {
                    rpr_depth -= 1;
                }
                index += 1;
            }
            Event::Text(e) => {
                let decoded = e
                    .decode()
                    .map_err(|error| format!("解析 document.xml 文本失败：{error}"))?;
                if in_text {
                    buffer.push_str(&decoded);
                } else if !decoded.trim().is_empty() {
                    return Err("当前 docx 运行节点中存在游离文本，无法安全写回。".to_string());
                }
                index += 1;
            }
            Event::CData(e) => {
                let decoded = e
                    .decode()
                    .map_err(|error| format!("解析 document.xml CDATA 失败：{error}"))?;
                if in_text {
                    buffer.push_str(&decoded);
                } else if !decoded.trim().is_empty() {
                    return Err("当前 docx 运行节点中存在游离 CDATA，无法安全写回。".to_string());
                }
                index += 1;
            }
            Event::Comment(_)
            | Event::Decl(_)
            | Event::PI(_)
            | Event::DocType(_)
            | Event::GeneralRef(_)
            | Event::Eof => index += 1,
        }
    }

    flush_writeback_editable_region(
        &mut regions,
        &mut buffer,
        presentation,
        hyperlink_start.cloned(),
        &run_property_events,
    );
    Ok(regions)
}

fn collect_run_property_events(events: &[Event<'static>]) -> Vec<Event<'static>> {
    let mut out = Vec::new();
    let mut depth = 0usize;

    for event in events.iter().skip(1) {
        match event {
            Event::Start(e) => {
                let name = local_name_owned(e.name().as_ref());
                if name.as_slice() == b"rPr" || depth > 0 {
                    depth += 1;
                    out.push(Event::Start(e.clone()));
                }
            }
            Event::Empty(e) => {
                let name = local_name_owned(e.name().as_ref());
                if name.as_slice() == b"rPr" || depth > 0 {
                    out.push(Event::Empty(e.clone()));
                }
            }
            Event::End(e) => {
                if depth > 0 {
                    out.push(Event::End(e.clone()));
                    depth -= 1;
                }
            }
            Event::Text(e) => {
                if depth > 0 {
                    out.push(Event::Text(e.clone()));
                }
            }
            Event::CData(e) => {
                if depth > 0 {
                    out.push(Event::CData(e.clone()));
                }
            }
            Event::Comment(e) => {
                if depth > 0 {
                    out.push(Event::Comment(e.clone()));
                }
            }
            Event::Decl(e) => {
                if depth > 0 {
                    out.push(Event::Decl(e.clone()));
                }
            }
            Event::PI(e) => {
                if depth > 0 {
                    out.push(Event::PI(e.clone()));
                }
            }
            Event::DocType(e) => {
                if depth > 0 {
                    out.push(Event::DocType(e.clone()));
                }
            }
            Event::GeneralRef(e) => {
                if depth > 0 {
                    out.push(Event::GeneralRef(e.clone()));
                }
            }
            Event::Eof => {}
        }
    }

    out
}

fn build_run_presentation(
    run_property_events: &[Event<'static>],
    href: Option<String>,
    hyperlink_signature: Option<&str>,
) -> Option<ChunkPresentation> {
    let mut style = RunStyle::default();
    for event in run_property_events {
        if let Event::Start(e) | Event::Empty(e) = event {
            update_run_style(&mut style, e);
        }
    }
    current_editable_presentation(
        &style,
        href,
        current_run_writeback_key(run_property_events, hyperlink_signature),
    )
}

fn flush_writeback_editable_region(
    regions: &mut Vec<WritebackRegionTemplate>,
    buffer: &mut String,
    presentation: Option<ChunkPresentation>,
    hyperlink_start: Option<BytesStart<'static>>,
    run_property_events: &[Event<'static>],
) {
    if buffer.is_empty() {
        return;
    }
    let render = match hyperlink_start {
        Some(hyperlink_start) => EditableRegionRender::Hyperlink {
            hyperlink_start,
            run_property_events: run_property_events.to_vec(),
        },
        None => EditableRegionRender::Run {
            run_property_events: run_property_events.to_vec(),
        },
    };
    regions.push(WritebackRegionTemplate::Editable(EditableRegionTemplate {
        text: std::mem::take(buffer),
        presentation,
        render,
    }));
}

fn parse_writeback_formula_region(
    events: &[Event<'static>],
) -> Result<WritebackRegionTemplate, String> {
    let mut text = String::new();
    let mut math_text_depth = 0usize;
    for event in events {
        match event {
            Event::Start(e) => {
                if local_name(e.name().as_ref()) == b"t" {
                    math_text_depth += 1;
                }
            }
            Event::End(e) => {
                if local_name(e.name().as_ref()) == b"t" && math_text_depth > 0 {
                    math_text_depth -= 1;
                }
            }
            Event::Text(e) => {
                if math_text_depth > 0 {
                    let decoded = e
                        .decode()
                        .map_err(|error| format!("解析数学公式文本失败：{error}"))?;
                    let trimmed = decoded.trim();
                    if !trimmed.is_empty() {
                        text.push_str(trimmed);
                    }
                }
            }
            Event::CData(e) => {
                if math_text_depth > 0 {
                    let decoded = e
                        .decode()
                        .map_err(|error| format!("解析数学公式 CDATA 失败：{error}"))?;
                    let trimmed = decoded.trim();
                    if !trimmed.is_empty() {
                        text.push_str(trimmed);
                    }
                }
            }
            Event::Empty(_)
            | Event::Comment(_)
            | Event::Decl(_)
            | Event::PI(_)
            | Event::DocType(_)
            | Event::GeneralRef(_)
            | Event::Eof => {}
        }
    }

    Ok(placeholders::raw_locked_region(&text, "formula", events))
}

fn split_updated_paragraphs(
    updated_text: &str,
    expected_count: usize,
) -> Result<Vec<String>, String> {
    let normalized = updated_text.replace("\r\n", "\n").replace('\r', "\n");

    let paragraphs = if expected_count == 0 && normalized.is_empty() {
        Vec::new()
    } else {
        normalized
            .split("\n\n")
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
    };

    if paragraphs.len() != expected_count {
        return Err(
            "当前简单 docx 写回要求段落数量保持不变，请不要新增、删除或合并段落。".to_string(),
        );
    }
    Ok(paragraphs)
}

fn build_plain_text_editor_updated_regions(
    blocks: &[WritebackBlockTemplate],
    updated_text: &str,
) -> Result<Vec<TextRegion>, String> {
    validate_plain_text_editor_blocks(blocks)?;
    let updated_paragraphs = split_updated_paragraphs(updated_text, blocks.len())?;
    let mut regions = Vec::new();

    for (index, block) in blocks.iter().enumerate() {
        let updated_paragraph = updated_paragraphs
            .get(index)
            .ok_or_else(|| "docx 段落数量与写回内容不一致，无法生成 document.xml。".to_string())?
            .clone();
        let append_block_separator = index + 1 < blocks.len();
        let mut block_regions = match block {
            WritebackBlockTemplate::Paragraph(paragraph) => {
                build_plain_text_editor_paragraph_regions(paragraph, &updated_paragraph)?
            }
            WritebackBlockTemplate::Locked(region) => vec![TextRegion {
                body: updated_paragraph,
                skip_rewrite: true,
                presentation: region.presentation.clone(),
            }],
        };
        if let Some(last) = block_regions.last_mut() {
            if append_block_separator {
                last.body.push_str(DOCX_BLOCK_SEPARATOR);
            }
        }
        regions.extend(block_regions);
    }

    Ok(regions)
}

fn validate_plain_text_editor_blocks(blocks: &[WritebackBlockTemplate]) -> Result<(), String> {
    for block in blocks {
        if let WritebackBlockTemplate::Paragraph(paragraph) = block {
            validate_plain_text_editor_paragraph(paragraph)?;
        }
    }
    Ok(())
}

fn build_plain_text_editor_paragraph_regions(
    paragraph: &WritebackParagraphTemplate,
    updated_text: &str,
) -> Result<Vec<TextRegion>, String> {
    validate_plain_text_editor_paragraph(paragraph)?;
    if paragraph.regions.is_empty() {
        return Ok(vec![TextRegion {
            body: updated_text.to_string(),
            skip_rewrite: false,
            presentation: None,
        }]);
    }

    let mut matches = Vec::new();
    collect_plain_text_editor_paragraph_matches(
        &paragraph.regions,
        updated_text,
        0,
        0,
        Vec::new(),
        &mut matches,
        2,
    );

    match matches.len() {
        1 => Ok(matches.remove(0)),
        0 => Err(
            "纯文本编辑器中的锁定内容已变化，或文本已越过原有样式边界，无法安全写回。".to_string(),
        ),
        _ => Err("纯文本编辑器中的锁定内容定位存在歧义，无法安全写回。".to_string()),
    }
}

fn validate_plain_text_editor_paragraph(
    _paragraph: &WritebackParagraphTemplate,
) -> Result<(), String> {
    Ok(())
}

fn collect_plain_text_editor_paragraph_matches(
    template: &[WritebackRegionTemplate],
    updated_text: &str,
    region_index: usize,
    cursor: usize,
    current: Vec<TextRegion>,
    matches: &mut Vec<Vec<TextRegion>>,
    limit: usize,
) {
    if matches.len() >= limit {
        return;
    }
    if region_index >= template.len() {
        if cursor == updated_text.len() {
            matches.push(current);
        }
        return;
    }

    match &template[region_index] {
        WritebackRegionTemplate::Editable(_) => collect_plain_text_editor_editable_group_matches(
            template,
            updated_text,
            region_index,
            cursor,
            current,
            matches,
            limit,
        ),
        WritebackRegionTemplate::Locked(region) => {
            let Some(rest) = updated_text.get(cursor..) else {
                return;
            };
            if !rest.starts_with(&region.text) {
                return;
            }
            let mut next = current;
            next.push(TextRegion {
                body: region.text.clone(),
                skip_rewrite: true,
                presentation: region.presentation.clone(),
            });
            collect_plain_text_editor_paragraph_matches(
                template,
                updated_text,
                region_index + 1,
                cursor + region.text.len(),
                next,
                matches,
                limit,
            );
        }
    }
}

#[derive(Default)]
struct PlainTextEditorEditBlock {
    owner: Option<usize>,
    ambiguous: bool,
    inserted_text: String,
}

fn collect_plain_text_editor_editable_group_matches(
    template: &[WritebackRegionTemplate],
    updated_text: &str,
    region_index: usize,
    cursor: usize,
    current: Vec<TextRegion>,
    matches: &mut Vec<Vec<TextRegion>>,
    limit: usize,
) {
    let group_end = next_locked_region_index(template, region_index);
    let group = &template[region_index..group_end];

    match template.get(group_end) {
        Some(WritebackRegionTemplate::Locked(next_locked)) => {
            if next_locked.text.is_empty() {
                return;
            }
            let Some(rest) = updated_text.get(cursor..) else {
                return;
            };
            for (relative_index, _) in rest.match_indices(&next_locked.text) {
                let next_cursor = cursor + relative_index;
                let Some(mapped) =
                    map_editable_group_regions(group, &updated_text[cursor..next_cursor])
                else {
                    continue;
                };
                let mut next = current.clone();
                next.extend(mapped);
                collect_plain_text_editor_paragraph_matches(
                    template,
                    updated_text,
                    group_end,
                    next_cursor,
                    next,
                    matches,
                    limit,
                );
                if matches.len() >= limit {
                    return;
                }
            }
        }
        None => {
            let Some(mapped) = map_editable_group_regions(group, &updated_text[cursor..]) else {
                return;
            };
            let mut completed = current;
            completed.extend(mapped);
            matches.push(completed);
        }
        Some(WritebackRegionTemplate::Editable(_)) => {}
    }
}

fn next_locked_region_index(template: &[WritebackRegionTemplate], start: usize) -> usize {
    let mut index = start;
    while index < template.len() {
        if matches!(template[index], WritebackRegionTemplate::Locked(_)) {
            return index;
        }
        index += 1;
    }
    template.len()
}

fn map_editable_group_regions(
    group: &[WritebackRegionTemplate],
    updated_text: &str,
) -> Option<Vec<TextRegion>> {
    let editable_regions = editable_group_templates(group)?;
    if editable_regions.len() == 1 {
        return Some(vec![editable_region_text(
            editable_regions[0],
            updated_text.to_string(),
        )]);
    }

    let original_text = editable_regions
        .iter()
        .map(|region| region.text.as_str())
        .collect::<String>();
    if original_text.is_empty() {
        return map_empty_editable_group_regions(&editable_regions, updated_text);
    }

    let boundaries = editable_group_char_boundaries(&editable_regions);
    let original_len = original_text.chars().count();
    let mut outputs = vec![String::new(); editable_regions.len()];
    let mut original_index = 0usize;
    let mut block = PlainTextEditorEditBlock::default();

    for span in crate::rewrite::build_diff(&original_text, updated_text) {
        if !apply_diff_span_to_editable_group(
            &mut outputs,
            &mut block,
            &boundaries,
            original_len,
            &mut original_index,
            span.r#type,
            &span.text,
        ) {
            return None;
        }
    }

    if !flush_plain_text_editor_edit_block(&mut outputs, &mut block) {
        return None;
    }
    if original_index != original_len {
        return None;
    }

    Some(
        outputs
            .into_iter()
            .zip(editable_regions)
            .map(|(body, region)| editable_region_text(region, body))
            .collect(),
    )
}

fn editable_group_templates(
    group: &[WritebackRegionTemplate],
) -> Option<Vec<&EditableRegionTemplate>> {
    group
        .iter()
        .map(|region| match region {
            WritebackRegionTemplate::Editable(region) => Some(region),
            WritebackRegionTemplate::Locked(_) => None,
        })
        .collect()
}

fn map_empty_editable_group_regions(
    editable_regions: &[&EditableRegionTemplate],
    updated_text: &str,
) -> Option<Vec<TextRegion>> {
    if updated_text.is_empty() {
        return Some(
            editable_regions
                .iter()
                .map(|region| editable_region_text(region, String::new()))
                .collect(),
        );
    }
    if editable_regions.len() == 1 {
        return Some(vec![editable_region_text(
            editable_regions[0],
            updated_text.to_string(),
        )]);
    }
    None
}

fn editable_group_char_boundaries(editable_regions: &[&EditableRegionTemplate]) -> Vec<usize> {
    let mut boundaries = Vec::with_capacity(editable_regions.len());
    let mut total = 0usize;
    for region in editable_regions {
        total += region.text.chars().count();
        boundaries.push(total);
    }
    boundaries
}

fn apply_diff_span_to_editable_group(
    outputs: &mut [String],
    block: &mut PlainTextEditorEditBlock,
    boundaries: &[usize],
    original_len: usize,
    original_index: &mut usize,
    diff_type: DiffType,
    text: &str,
) -> bool {
    match diff_type {
        DiffType::Unchanged => {
            apply_unchanged_editable_text(outputs, block, boundaries, original_index, text)
        }
        DiffType::Delete => apply_deleted_editable_text(block, boundaries, original_index, text),
        DiffType::Insert => {
            apply_inserted_editable_text(block, boundaries, original_len, *original_index, text)
        }
    }
}

fn apply_unchanged_editable_text(
    outputs: &mut [String],
    block: &mut PlainTextEditorEditBlock,
    boundaries: &[usize],
    original_index: &mut usize,
    text: &str,
) -> bool {
    if !flush_plain_text_editor_edit_block(outputs, block) {
        return false;
    }
    for ch in text.chars() {
        let Some(region_index) = region_index_for_original_char(boundaries, *original_index) else {
            return false;
        };
        outputs[region_index].push(ch);
        *original_index += 1;
    }
    true
}

fn apply_deleted_editable_text(
    block: &mut PlainTextEditorEditBlock,
    boundaries: &[usize],
    original_index: &mut usize,
    text: &str,
) -> bool {
    for _ in text.chars() {
        let Some(region_index) = region_index_for_original_char(boundaries, *original_index) else {
            return false;
        };
        add_plain_text_editor_block_owner(block, region_index);
        *original_index += 1;
    }
    true
}

fn apply_inserted_editable_text(
    block: &mut PlainTextEditorEditBlock,
    boundaries: &[usize],
    original_len: usize,
    original_index: usize,
    text: &str,
) -> bool {
    let Some(region_index) = insertion_region_index(boundaries, original_len, original_index)
    else {
        return false;
    };
    add_plain_text_editor_block_owner(block, region_index);
    block.inserted_text.push_str(text);
    true
}

fn flush_plain_text_editor_edit_block(
    outputs: &mut [String],
    block: &mut PlainTextEditorEditBlock,
) -> bool {
    if block.ambiguous {
        return false;
    }
    if let Some(owner) = block.owner {
        outputs[owner].push_str(&block.inserted_text);
    } else if !block.inserted_text.is_empty() {
        return false;
    }
    *block = PlainTextEditorEditBlock::default();
    true
}

fn add_plain_text_editor_block_owner(block: &mut PlainTextEditorEditBlock, owner: usize) {
    match block.owner {
        Some(current) if current != owner => block.ambiguous = true,
        Some(_) => {}
        None => block.owner = Some(owner),
    }
}

fn region_index_for_original_char(boundaries: &[usize], original_index: usize) -> Option<usize> {
    boundaries.iter().position(|end| original_index < *end)
}

fn insertion_region_index(
    boundaries: &[usize],
    original_len: usize,
    original_index: usize,
) -> Option<usize> {
    if boundaries.is_empty() {
        return None;
    }
    if original_index == 0 {
        return Some(0);
    }
    if original_index == original_len {
        return Some(boundaries.len() - 1);
    }

    let left = region_index_for_original_char(boundaries, original_index - 1)?;
    let right = region_index_for_original_char(boundaries, original_index)?;
    if left == right {
        Some(left)
    } else {
        None
    }
}

fn editable_region_text(region: &EditableRegionTemplate, body: String) -> TextRegion {
    TextRegion {
        body,
        skip_rewrite: false,
        presentation: region.presentation.clone(),
    }
}

fn rewrite_document_xml_with_regions(
    xml: &str,
    blocks: &[WritebackBlockTemplate],
    updated_regions: &[TextRegion],
) -> Result<String, String> {
    let paragraph_updates = collect_updated_writeback_regions(blocks, updated_regions)?;
    let paragraphs = blocks
        .iter()
        .filter_map(|block| match block {
            WritebackBlockTemplate::Paragraph(paragraph) => Some(paragraph),
            WritebackBlockTemplate::Locked(_) => None,
        })
        .collect::<Vec<_>>();

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut writer = Writer::new(Vec::new());
    let mut buf = Vec::new();
    let mut body_depth = 0usize;
    let mut paragraph_depth = 0usize;
    let mut paragraph_index = 0usize;

    loop {
        let event = match reader.read_event_into(&mut buf) {
            Ok(event) => event.into_owned(),
            Err(error) => return Err(format!("解析 document.xml 失败：{error}")),
        };

        match event {
            Event::Start(e) => {
                let name = local_name_owned(e.name().as_ref());
                if paragraph_depth > 0 {
                    paragraph_depth += 1;
                } else if body_depth > 0 && body_depth == 1 && name.as_slice() == b"p" {
                    paragraph_depth = 1;
                } else {
                    writer
                        .write_event(Event::Start(e.clone()))
                        .map_err(|error| format!("生成 document.xml 失败：{error}"))?;
                    if name.as_slice() == b"body" {
                        body_depth = 1;
                    } else if body_depth > 0 {
                        body_depth += 1;
                    }
                }
            }
            Event::Empty(e) => {
                let name = local_name_owned(e.name().as_ref());
                if paragraph_depth > 0 {
                    // paragraph subtree will be rewritten when the outer <w:p> closes
                } else if body_depth > 0 && body_depth == 1 && name.as_slice() == b"p" {
                    let paragraph = paragraphs.get(paragraph_index).ok_or_else(|| {
                        "docx 段落数量与写回模板不一致，无法生成 document.xml。".to_string()
                    })?;
                    let updated = paragraph_updates.get(paragraph_index).ok_or_else(|| {
                        "docx 段落数量与写回内容不一致，无法生成 document.xml。".to_string()
                    })?;
                    write_rewritten_paragraph_from_template(&mut writer, paragraph, updated)?;
                    paragraph_index += 1;
                } else {
                    writer
                        .write_event(Event::Empty(e.clone()))
                        .map_err(|error| format!("生成 document.xml 失败：{error}"))?;
                }
            }
            Event::End(e) => {
                let name = local_name_owned(e.name().as_ref());
                if paragraph_depth > 0 {
                    paragraph_depth -= 1;
                    if paragraph_depth == 0 {
                        let paragraph = paragraphs.get(paragraph_index).ok_or_else(|| {
                            "docx 段落数量与写回模板不一致，无法生成 document.xml。".to_string()
                        })?;
                        let updated = paragraph_updates.get(paragraph_index).ok_or_else(|| {
                            "docx 段落数量与写回内容不一致，无法生成 document.xml。".to_string()
                        })?;
                        write_rewritten_paragraph_from_template(&mut writer, paragraph, updated)?;
                        paragraph_index += 1;
                    }
                } else {
                    writer
                        .write_event(Event::End(e.clone()))
                        .map_err(|error| format!("生成 document.xml 失败：{error}"))?;
                    if body_depth > 0 {
                        if name.as_slice() == b"body" && body_depth == 1 {
                            body_depth = 0;
                        } else {
                            body_depth -= 1;
                        }
                    }
                }
            }
            Event::Text(e) => {
                if paragraph_depth == 0 {
                    writer
                        .write_event(Event::Text(e.clone()))
                        .map_err(|error| format!("生成 document.xml 失败：{error}"))?;
                }
            }
            Event::CData(e) => {
                if paragraph_depth == 0 {
                    writer
                        .write_event(Event::CData(e.clone()))
                        .map_err(|error| format!("生成 document.xml 失败：{error}"))?;
                }
            }
            Event::Comment(e) => {
                if paragraph_depth == 0 {
                    writer
                        .write_event(Event::Comment(e.clone()))
                        .map_err(|error| format!("生成 document.xml 失败：{error}"))?;
                }
            }
            Event::Decl(e) => {
                if paragraph_depth == 0 {
                    writer
                        .write_event(Event::Decl(e.clone()))
                        .map_err(|error| format!("生成 document.xml 失败：{error}"))?;
                }
            }
            Event::PI(e) => {
                if paragraph_depth == 0 {
                    writer
                        .write_event(Event::PI(e.clone()))
                        .map_err(|error| format!("生成 document.xml 失败：{error}"))?;
                }
            }
            Event::DocType(e) => {
                if paragraph_depth == 0 {
                    writer
                        .write_event(Event::DocType(e.clone()))
                        .map_err(|error| format!("生成 document.xml 失败：{error}"))?;
                }
            }
            Event::GeneralRef(e) => {
                if paragraph_depth == 0 {
                    writer
                        .write_event(Event::GeneralRef(e.clone()))
                        .map_err(|error| format!("生成 document.xml 失败：{error}"))?;
                }
            }
            Event::Eof => break,
        }

        buf.clear();
    }

    if paragraph_index != paragraphs.len() {
        return Err("docx 段落数量与写回模板不一致，无法生成 document.xml。".to_string());
    }

    String::from_utf8(writer.into_inner())
        .map_err(|error| format!("生成 document.xml 失败：{error}"))
}

fn collect_updated_writeback_regions(
    blocks: &[WritebackBlockTemplate],
    updated_regions: &[TextRegion],
) -> Result<Vec<Vec<String>>, String> {
    let mut updated_index = 0usize;
    let mut paragraph_updates = Vec::new();

    for (block_index, block) in blocks.iter().enumerate() {
        let append_block_separator = block_index + 1 < blocks.len();
        match block {
            WritebackBlockTemplate::Paragraph(paragraph) => {
                if paragraph.regions.is_empty() {
                    let updated = updated_regions.get(updated_index).ok_or_else(|| {
                        "写回内容与原 docx 结构不一致：区域数量不足。".to_string()
                    })?;
                    let expected_body = if append_block_separator {
                        DOCX_BLOCK_SEPARATOR.to_string()
                    } else {
                        String::new()
                    };
                    validate_updated_empty_paragraph_region(updated, &expected_body)?;
                    updated_index += 1;
                    paragraph_updates.push(Vec::new());
                    continue;
                }
                let mut region_updates = Vec::new();
                for (region_index, region) in paragraph.regions.iter().enumerate() {
                    let updated = updated_regions.get(updated_index).ok_or_else(|| {
                        "写回内容与原 docx 结构不一致：区域数量不足。".to_string()
                    })?;
                    let expected_separator =
                        append_block_separator && region_index + 1 == paragraph.regions.len();
                    let expected_body = if expected_separator {
                        format!("{}{}", region.text(), DOCX_BLOCK_SEPARATOR)
                    } else {
                        region.text().to_string()
                    };
                    validate_updated_region(region, updated, &expected_body, paragraph.is_heading)?;
                    let cleaned = if expected_separator {
                        updated
                            .body
                            .strip_suffix(DOCX_BLOCK_SEPARATOR)
                            .ok_or_else(|| {
                                "写回内容与原 docx 结构不一致：块分隔符丢失。".to_string()
                            })?
                            .to_string()
                    } else {
                        updated.body.clone()
                    };
                    region_updates.push(cleaned);
                    updated_index += 1;
                }
                paragraph_updates.push(region_updates);
            }
            WritebackBlockTemplate::Locked(region) => {
                let updated = updated_regions
                    .get(updated_index)
                    .ok_or_else(|| "写回内容与原 docx 结构不一致：区域数量不足。".to_string())?;
                let expected_body = if append_block_separator {
                    format!("{}{}", region.text, DOCX_BLOCK_SEPARATOR)
                } else {
                    region.text.clone()
                };
                validate_updated_locked_region(region, updated, &expected_body)?;
                updated_index += 1;
            }
        }
    }

    if updated_index != updated_regions.len() {
        return Err("写回内容与原 docx 结构不一致：区域数量过多。".to_string());
    }

    Ok(paragraph_updates)
}

fn validate_updated_empty_paragraph_region(
    updated: &TextRegion,
    expected_body: &str,
) -> Result<(), String> {
    if updated.body != expected_body {
        return Err("写回内容与原 docx 结构不一致：空段落边界已变化。".to_string());
    }
    if updated.presentation.is_some() {
        return Err("写回内容与原 docx 结构不一致：空段落不应带有行内样式或链接。".to_string());
    }
    Ok(())
}

fn validate_updated_region(
    expected: &WritebackRegionTemplate,
    updated: &TextRegion,
    expected_body: &str,
    allow_locked_editable_region: bool,
) -> Result<(), String> {
    if !expected.skip_rewrite()
        && allow_locked_editable_region
        && updated.skip_rewrite
        && expected.presentation() == updated.presentation.as_ref()
    {
        if updated.body != expected_body {
            return Err("写回内容改动了锁定标题内容，已拒绝写回。".to_string());
        }
        return Ok(());
    }
    if expected.skip_rewrite() != updated.skip_rewrite
        || expected.presentation() != updated.presentation.as_ref()
    {
        return Err(
            "写回内容与原 docx 结构不一致：行内样式、超链接或锁定区边界已变化。".to_string(),
        );
    }
    if expected.skip_rewrite() && updated.body != expected_body {
        return Err("写回内容改动了锁定内容（例如公式、分页符或占位符），已拒绝写回。".to_string());
    }
    Ok(())
}

fn validate_updated_locked_region(
    expected: &LockedRegionTemplate,
    updated: &TextRegion,
    expected_body: &str,
) -> Result<(), String> {
    if !updated.skip_rewrite || updated.presentation.as_ref() != expected.presentation.as_ref() {
        return Err("写回内容与原 docx 结构不一致：锁定区元数据已变化。".to_string());
    }
    if updated.body != expected_body {
        return Err("写回内容改动了锁定内容（例如公式、分页符或占位符），已拒绝写回。".to_string());
    }
    Ok(())
}

fn write_rewritten_paragraph_from_template(
    writer: &mut Writer<Vec<u8>>,
    paragraph: &WritebackParagraphTemplate,
    updated_regions: &[String],
) -> Result<(), String> {
    writer
        .write_event(Event::Start(paragraph.paragraph_start.clone()))
        .map_err(|error| format!("生成 document.xml 失败：{error}"))?;

    for event in &paragraph.paragraph_property_events {
        writer
            .write_event(event.clone())
            .map_err(|error| format!("生成 document.xml 失败：{error}"))?;
    }

    for (region, updated_text) in paragraph.regions.iter().zip(updated_regions.iter()) {
        match region {
            WritebackRegionTemplate::Editable(region) => {
                write_editable_region(writer, region, updated_text)?;
            }
            WritebackRegionTemplate::Locked(region) => {
                write_locked_region(writer, region)?;
            }
        }
    }

    writer
        .write_event(Event::End(paragraph.paragraph_end.clone()))
        .map_err(|error| format!("生成 document.xml 失败：{error}"))?;
    Ok(())
}

fn write_editable_region(
    writer: &mut Writer<Vec<u8>>,
    region: &EditableRegionTemplate,
    updated_text: &str,
) -> Result<(), String> {
    if updated_text.is_empty() {
        return Ok(());
    }
    match &region.render {
        EditableRegionRender::Run {
            run_property_events,
        } => write_styled_text_run(writer, run_property_events, updated_text),
        EditableRegionRender::Hyperlink {
            hyperlink_start,
            run_property_events,
        } => {
            writer
                .write_event(Event::Start(hyperlink_start.clone()))
                .map_err(|error| format!("生成 document.xml 失败：{error}"))?;
            write_styled_text_run(writer, run_property_events, updated_text)?;
            writer
                .write_event(Event::End(hyperlink_start.to_end().into_owned()))
                .map_err(|error| format!("生成 document.xml 失败：{error}"))?;
            Ok(())
        }
    }
}

fn write_locked_region(
    writer: &mut Writer<Vec<u8>>,
    region: &LockedRegionTemplate,
) -> Result<(), String> {
    write_locked_region_render(writer, &region.render)
}

fn write_locked_region_render(
    writer: &mut Writer<Vec<u8>>,
    render: &LockedRegionRender,
) -> Result<(), String> {
    match render {
        LockedRegionRender::RawEvents(events) => write_raw_locked_events(writer, events),
        LockedRegionRender::PageBreak => write_locked_page_break(writer),
        LockedRegionRender::Sequence(items) => {
            for item in items {
                write_locked_region_render(writer, item)?;
            }
            Ok(())
        }
    }
}

fn write_raw_locked_events(
    writer: &mut Writer<Vec<u8>>,
    events: &[Event<'static>],
) -> Result<(), String> {
    for event in events {
        writer
            .write_event(event.clone())
            .map_err(|error| format!("生成 document.xml 失败：{error}"))?;
    }
    Ok(())
}

fn write_locked_page_break(writer: &mut Writer<Vec<u8>>) -> Result<(), String> {
    writer
        .write_event(Event::Start(BytesStart::new("w:r")))
        .map_err(|error| format!("生成 document.xml 失败：{error}"))?;
    let mut br = BytesStart::new("w:br");
    br.push_attribute(("w:type", "page"));
    writer
        .write_event(Event::Empty(br))
        .map_err(|error| format!("生成 document.xml 失败：{error}"))?;
    writer
        .write_event(Event::End(BytesEnd::new("w:r")))
        .map_err(|error| format!("生成 document.xml 失败：{error}"))?;
    Ok(())
}

fn paragraph_bounds(
    events: &[Event<'static>],
) -> Result<(BytesStart<'static>, BytesEnd<'static>), String> {
    let Some(first) = events.first() else {
        return Err("生成 document.xml 失败：段落事件为空。".to_string());
    };

    match first {
        Event::Start(e) | Event::Empty(e) => Ok((e.clone(), e.to_end().into_owned())),
        _ => Err("生成 document.xml 失败：段落起始事件非法。".to_string()),
    }
}

fn collect_paragraph_property_events(events: &[Event<'static>]) -> Vec<Event<'static>> {
    let mut out = Vec::new();
    let mut ppr_depth = 0usize;

    for event in events.iter().skip(1) {
        match event {
            Event::Start(e) => {
                let name = local_name_owned(e.name().as_ref());
                if name.as_slice() == b"pPr" || ppr_depth > 0 {
                    ppr_depth += 1;
                    out.push(Event::Start(e.clone()));
                }
            }
            Event::Empty(e) => {
                let name = local_name_owned(e.name().as_ref());
                if name.as_slice() == b"pPr" || ppr_depth > 0 {
                    out.push(Event::Empty(e.clone()));
                }
            }
            Event::End(e) => {
                if ppr_depth > 0 {
                    out.push(Event::End(e.clone()));
                    ppr_depth -= 1;
                }
            }
            Event::Text(e) => {
                if ppr_depth > 0 {
                    out.push(Event::Text(e.clone()));
                }
            }
            Event::CData(e) => {
                if ppr_depth > 0 {
                    out.push(Event::CData(e.clone()));
                }
            }
            Event::Comment(e) => {
                if ppr_depth > 0 {
                    out.push(Event::Comment(e.clone()));
                }
            }
            Event::Decl(e) => {
                if ppr_depth > 0 {
                    out.push(Event::Decl(e.clone()));
                }
            }
            Event::PI(e) => {
                if ppr_depth > 0 {
                    out.push(Event::PI(e.clone()));
                }
            }
            Event::DocType(e) => {
                if ppr_depth > 0 {
                    out.push(Event::DocType(e.clone()));
                }
            }
            Event::GeneralRef(e) => {
                if ppr_depth > 0 {
                    out.push(Event::GeneralRef(e.clone()));
                }
            }
            Event::Eof => {}
        }
    }

    out
}

fn paragraph_properties_indicate_heading(events: &[Event<'static>]) -> bool {
    events.iter().any(|event| match event {
        Event::Start(e) | Event::Empty(e) => {
            local_name(e.name().as_ref()) == b"pStyle"
                && attr_value(e, b"val").is_some_and(|value| is_heading_style_id(&value))
        }
        _ => false,
    })
}

fn write_styled_text_run(
    writer: &mut Writer<Vec<u8>>,
    run_property_events: &[Event<'static>],
    text: &str,
) -> Result<(), String> {
    writer
        .write_event(Event::Start(BytesStart::new("w:r")))
        .map_err(|error| format!("生成 document.xml 失败：{error}"))?;

    for event in run_property_events {
        writer
            .write_event(event.clone())
            .map_err(|error| format!("生成 document.xml 失败：{error}"))?;
    }

    let mut buffer = String::new();
    for ch in text.chars() {
        if ch == '\t' {
            write_run_text_segment(writer, &mut buffer)?;
            writer
                .write_event(Event::Empty(BytesStart::new("w:tab")))
                .map_err(|error| format!("生成 document.xml 失败：{error}"))?;
            continue;
        }
        if ch == '\n' {
            write_run_text_segment(writer, &mut buffer)?;
            writer
                .write_event(Event::Empty(BytesStart::new("w:br")))
                .map_err(|error| format!("生成 document.xml 失败：{error}"))?;
            continue;
        }
        buffer.push(ch);
    }
    write_run_text_segment(writer, &mut buffer)?;

    writer
        .write_event(Event::End(BytesEnd::new("w:r")))
        .map_err(|error| format!("生成 document.xml 失败：{error}"))?;
    Ok(())
}

fn write_run_text_segment(writer: &mut Writer<Vec<u8>>, buffer: &mut String) -> Result<(), String> {
    if buffer.is_empty() {
        return Ok(());
    }

    let mut text_start = BytesStart::new("w:t");
    if needs_space_preserve(buffer) {
        text_start.push_attribute(("xml:space", "preserve"));
    }
    writer
        .write_event(Event::Start(text_start))
        .map_err(|error| format!("生成 document.xml 失败：{error}"))?;
    writer
        .write_event(Event::Text(BytesText::new(buffer)))
        .map_err(|error| format!("生成 document.xml 失败：{error}"))?;
    writer
        .write_event(Event::End(BytesEnd::new("w:t")))
        .map_err(|error| format!("生成 document.xml 失败：{error}"))?;
    buffer.clear();
    Ok(())
}

fn needs_space_preserve(text: &str) -> bool {
    text.chars().next().is_some_and(char::is_whitespace)
        || text.chars().last().is_some_and(char::is_whitespace)
}

fn replace_document_xml(docx_bytes: &[u8], updated_xml: &str) -> Result<Vec<u8>, String> {
    let cursor = Cursor::new(docx_bytes);
    let mut archive = ZipArchive::new(cursor)
        .map_err(|error| format!("无法解析 docx（zip 结构错误）：{error}"))?;
    let mut out = Cursor::new(Vec::new());
    let mut writer = ZipWriter::new(&mut out);

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("读取 docx 条目失败：{error}"))?;
        let name = entry.name().to_string();

        if entry.is_dir() {
            writer
                .add_directory(name, FileOptions::<()>::default())
                .map_err(|error| format!("重建 docx 目录失败：{error}"))?;
            continue;
        }

        writer
            .start_file(name.clone(), FileOptions::<()>::default())
            .map_err(|error| format!("重建 docx 条目失败：{error}"))?;
        if name == "word/document.xml" {
            writer
                .write_all(updated_xml.as_bytes())
                .map_err(|error| format!("写入 document.xml 失败：{error}"))?;
        } else {
            let mut payload = Vec::new();
            entry
                .read_to_end(&mut payload)
                .map_err(|error| format!("读取 docx 条目内容失败：{error}"))?;
            writer
                .write_all(&payload)
                .map_err(|error| format!("写入 docx 条目内容失败：{error}"))?;
        }
    }

    writer
        .finish()
        .map_err(|error| format!("完成 docx 写回失败：{error}"))?;
    Ok(out.into_inner())
}

fn tag_name(name: &[u8]) -> String {
    String::from_utf8_lossy(name).into_owned()
}

fn is_listish_paragraph(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with("- ") || trimmed.starts_with("• ")
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
