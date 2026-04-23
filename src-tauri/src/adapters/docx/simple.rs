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
    display::{build_display_blocks, DisplayBlockKind, DisplayBlockRef},
    model::{
        EditableRegionRender, EditableRegionTemplate, LockedDisplayMode, LockedRegionRender,
        LockedRegionTemplate, WritebackBlockTemplate, WritebackParagraphTemplate,
        WritebackRegionTemplate,
    },
    numbering::{list_marker_for_paragraph, NumberingTracker},
    package::{load_docx_document, DocxSupportData},
    placeholders,
    signature::build_docx_writeback_model,
    specials::{classify_block_sdt, classify_inline_special_region, is_inline_special_name},
    styles::ParagraphStyles,
    xml::{
        attr_value, hyperlink_target, local_name, local_name_owned, toggle_attr_enabled,
        underline_enabled,
    },
};
use crate::{
    adapters::TextRegion,
    models::{DiffType, TextPresentation},
    rewrite_unit::WritebackSlot,
};

/// Docx 适配器：从 `.docx`（Office Open XML）中抽取可改写的纯文本。
///
/// 重要说明：
/// - `.docx` 是 zip + XML 的二进制容器；
/// - 当前仅支持“简单 docx”：正文只包含普通段落/标题；
/// - 检测到复杂结构时会直接报错，不会带着不确定语义继续写回。
pub struct DocxAdapter;

impl DocxAdapter {
    pub(crate) fn load_writeback_source(
        docx_bytes: &[u8],
    ) -> Result<LoadedDocxWritebackSource, String> {
        load_docx_writeback_source(docx_bytes)
    }

    pub(crate) fn extract_writeback_model_from_source(
        loaded: &LoadedDocxWritebackSource,
        rewrite_headings: bool,
    ) -> super::signature::DocxWritebackModel {
        build_docx_writeback_model(&loaded.blocks, rewrite_headings)
    }

    #[cfg(test)]
    pub fn extract_text(docx_bytes: &[u8]) -> Result<String, String> {
        let loaded = load_docx_document(docx_bytes)?;
        let regions =
            extract_regions_from_document_xml(&loaded.document_xml, &loaded.support, true)?;
        Ok(regions
            .into_iter()
            .map(|region| region.body)
            .collect::<String>()
            .trim_matches('\u{feff}')
            .to_string())
    }

    #[cfg(test)]
    pub(crate) fn extract_writeback_source_text(docx_bytes: &[u8]) -> Result<String, String> {
        let loaded = load_docx_document(docx_bytes)?;
        let blocks = extract_writeback_paragraph_templates(&loaded.document_xml, &loaded.support)?;
        Ok(build_writeback_source_text(&blocks))
    }

    #[cfg(test)]
    pub fn extract_writeback_regions(docx_bytes: &[u8]) -> Result<Vec<TextRegion>, String> {
        let loaded = load_docx_document(docx_bytes)?;
        let blocks = extract_writeback_paragraph_templates(&loaded.document_xml, &loaded.support)?;
        Ok(flatten_writeback_blocks_for_test(&blocks))
    }

    #[cfg(test)]
    pub fn extract_regions(
        docx_bytes: &[u8],
        rewrite_headings: bool,
    ) -> Result<Vec<TextRegion>, String> {
        let loaded = load_docx_document(docx_bytes)?;
        extract_regions_from_document_xml(&loaded.document_xml, &loaded.support, rewrite_headings)
    }

    #[cfg(test)]
    pub fn extract_writeback_slots(
        docx_bytes: &[u8],
        rewrite_headings: bool,
    ) -> Result<Vec<crate::rewrite_unit::WritebackSlot>, String> {
        Ok(Self::extract_writeback_model(docx_bytes, rewrite_headings)?.writeback_slots)
    }

    #[cfg(test)]
    pub fn extract_writeback_model(
        docx_bytes: &[u8],
        rewrite_headings: bool,
    ) -> Result<super::signature::DocxWritebackModel, String> {
        let loaded = Self::load_writeback_source(docx_bytes)?;
        Ok(Self::extract_writeback_model_from_source(
            &loaded,
            rewrite_headings,
        ))
    }

    #[cfg(test)]
    pub fn write_updated_text(
        docx_bytes: &[u8],
        expected_source_text: &str,
        updated_text: &str,
    ) -> Result<Vec<u8>, String> {
        let loaded = Self::load_writeback_source(docx_bytes)?;
        Self::write_updated_text_with_source(docx_bytes, &loaded, expected_source_text, updated_text)
    }

    #[cfg(test)]
    pub fn write_updated_regions(
        docx_bytes: &[u8],
        expected_source_text: &str,
        updated_regions: &[TextRegion],
    ) -> Result<Vec<u8>, String> {
        let loaded = Self::load_writeback_source(docx_bytes)?;
        write_docx_with_regions(docx_bytes, &loaded, expected_source_text, updated_regions)
    }

    #[cfg(test)]
    pub fn write_updated_slots(
        docx_bytes: &[u8],
        expected_source_text: &str,
        updated_slots: &[WritebackSlot],
    ) -> Result<Vec<u8>, String> {
        let loaded = Self::load_writeback_source(docx_bytes)?;
        Self::write_updated_slots_with_source(docx_bytes, &loaded, expected_source_text, updated_slots)
    }

    pub(crate) fn write_updated_text_with_source(
        docx_bytes: &[u8],
        loaded: &LoadedDocxWritebackSource,
        expected_source_text: &str,
        updated_text: &str,
    ) -> Result<Vec<u8>, String> {
        let updated_regions = build_editor_writeback_updated_regions(&loaded.blocks, updated_text)?;
        write_docx_with_regions(docx_bytes, loaded, expected_source_text, &updated_regions)
    }

    pub(crate) fn write_updated_slots_with_source(
        docx_bytes: &[u8],
        loaded: &LoadedDocxWritebackSource,
        expected_source_text: &str,
        updated_slots: &[WritebackSlot],
    ) -> Result<Vec<u8>, String> {
        let updated_regions = text_regions_from_writeback_slots(updated_slots);
        write_docx_with_regions(docx_bytes, loaded, expected_source_text, &updated_regions)
    }

    #[cfg(test)]
    pub fn validate_writeback(docx_bytes: &[u8]) -> Result<(), String> {
        Self::load_writeback_source(docx_bytes).map(|_| ())
    }

    #[cfg(test)]
    pub fn validate_editor_writeback(docx_bytes: &[u8]) -> Result<(), String> {
        Self::validate_writeback(docx_bytes)
    }
}

const DOCX_BLOCK_SEPARATOR: &str = "\n\n";
const DOCX_PAGE_BREAK_PLACEHOLDER: &str = "[分页符]";
const DOCX_EMBEDDED_OBJECT_ERROR: &str =
    "当前不支持包含嵌入 Office 对象的 docx（例如 OLE、图表或 SmartArt）。";
const DOCX_HYPERLINK_PAGE_BREAK_ERROR: &str =
    "当前不支持超链接内分页符的 docx：这类结构无法安全写回，请先在 Word 中调整后再导入。";

pub(crate) struct LoadedDocxWritebackSource {
    document_xml: String,
    blocks: Vec<WritebackBlockTemplate>,
}

fn load_docx_writeback_source(docx_bytes: &[u8]) -> Result<LoadedDocxWritebackSource, String> {
    let loaded = load_docx_document(docx_bytes)?;
    let blocks = extract_writeback_paragraph_templates(&loaded.document_xml, &loaded.support)?;
    Ok(LoadedDocxWritebackSource {
        document_xml: loaded.document_xml,
        blocks,
    })
}

fn ensure_expected_docx_source_text(
    blocks: &[WritebackBlockTemplate],
    expected_source_text: &str,
) -> Result<(), String> {
    let current_source_text = build_writeback_source_text(blocks);
    if current_source_text == expected_source_text {
        return Ok(());
    }
    Err(
        "docx 原文件内容与当前会话不一致，文件可能已在外部发生变化。为避免误写，请重新导入。"
            .to_string(),
    )
}

fn write_docx_with_regions(
    docx_bytes: &[u8],
    loaded: &LoadedDocxWritebackSource,
    expected_source_text: &str,
    updated_regions: &[TextRegion],
) -> Result<Vec<u8>, String> {
    ensure_expected_docx_source_text(&loaded.blocks, expected_source_text)?;
    let updated_xml =
        rewrite_document_xml_with_regions(&loaded.document_xml, &loaded.blocks, updated_regions)?;
    replace_document_xml(docx_bytes, &updated_xml)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct RunStyle {
    bold: bool,
    italic: bool,
    underline: bool,
}

#[cfg(test)]
fn extract_regions_from_document_xml(
    xml: &str,
    support: &DocxSupportData,
    rewrite_headings: bool,
) -> Result<Vec<TextRegion>, String> {
    let blocks = extract_writeback_paragraph_templates(xml, support)?;
    Ok(flatten_writeback_blocks(&blocks, rewrite_headings))
}

fn text_regions_from_writeback_slots(updated_slots: &[WritebackSlot]) -> Vec<TextRegion> {
    let mut regions = Vec::new();
    let mut current_anchor: Option<&str> = None;
    let mut current_body = String::new();
    let mut current_presentation = None;
    let mut current_role = None;
    let mut current_has_editable = false;

    for slot in updated_slots {
        let anchor = slot.anchor.as_deref().unwrap_or(slot.id.as_str());
        if current_anchor.is_some_and(|current| current != anchor) {
            regions.push(slot_group_region(
                std::mem::take(&mut current_body),
                current_has_editable,
                current_role.take(),
                current_presentation.take(),
            ));
            current_has_editable = false;
        }

        if current_anchor != Some(anchor) {
            current_anchor = Some(anchor);
            current_presentation = slot.presentation.clone();
            current_role = Some(slot.role.clone());
        }

        current_body.push_str(&slot.text);
        current_body.push_str(&slot.separator_after);
        current_has_editable |= slot.editable;
    }

    if current_anchor.is_some() {
        regions.push(slot_group_region(
            current_body,
            current_has_editable,
            current_role,
            current_presentation,
        ));
    }

    regions
}

fn slot_group_region(
    body: String,
    editable: bool,
    role: Option<crate::rewrite_unit::WritebackSlotRole>,
    presentation: Option<TextPresentation>,
) -> TextRegion {
    if editable {
        return TextRegion::editable(body).with_presentation(presentation);
    }

    match role.unwrap_or(crate::rewrite_unit::WritebackSlotRole::LockedText) {
        crate::rewrite_unit::WritebackSlotRole::InlineObject => {
            TextRegion::inline_object(body).with_presentation(presentation)
        }
        crate::rewrite_unit::WritebackSlotRole::SyntaxToken => {
            TextRegion::syntax_token(body).with_presentation(presentation)
        }
        _ => locked_region_from_presentation(body, presentation),
    }
}

fn locked_region_from_presentation(
    body: String,
    presentation: Option<TextPresentation>,
) -> TextRegion {
    if presentation
        .as_ref()
        .and_then(|value| value.protect_kind.as_deref())
        .is_some()
    {
        return TextRegion::inline_object(body).with_presentation(presentation);
    }
    TextRegion::locked_text(body).with_presentation(presentation)
}

fn is_ignorable_paragraph_name(name: &[u8]) -> bool {
    matches!(
        name,
        b"bookmarkStart"
            | b"bookmarkEnd"
            | b"proofErr"
            | b"permStart"
            | b"permEnd"
            | b"commentRangeStart"
            | b"commentRangeEnd"
            | b"moveFromRangeStart"
            | b"moveFromRangeEnd"
            | b"moveToRangeStart"
            | b"moveToRangeEnd"
            | b"lastRenderedPageBreak"
    )
}

fn is_locked_inline_object_name(name: &[u8]) -> bool {
    is_inline_special_name(name)
}

fn writeback_locked_region_from_special(
    events: &[Event<'static>],
) -> Result<WritebackRegionTemplate, String> {
    let special = classify_inline_special_region(events)?;
    Ok(match special.display_mode {
        LockedDisplayMode::AfterParagraph => {
            placeholders::raw_locked_region_after_paragraph(&special.text, special.kind, events)
        }
        LockedDisplayMode::Inline => {
            placeholders::raw_locked_region(&special.text, special.kind, events)
        }
    })
}

fn writeback_locked_run_special_region(
    run_property_events: &[Event<'static>],
    child_events: &[Event<'static>],
) -> Result<WritebackRegionTemplate, String> {
    let special = classify_inline_special_region(child_events)?;
    Ok(placeholders::locked_run_child_region(
        &special.text,
        special.kind,
        run_property_events,
        child_events,
        special.display_mode,
    ))
}

fn special_run_char(event: &BytesStart<'_>) -> Option<char> {
    match local_name(event.name().as_ref()) {
        b"noBreakHyphen" => Some('\u{2011}'),
        b"softHyphen" => Some('\u{00ad}'),
        _ => None,
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
) -> Option<TextPresentation> {
    if !run_style.bold
        && !run_style.italic
        && !run_style.underline
        && href.is_none()
        && writeback_key.is_none()
    {
        return None;
    }
    Some(TextPresentation {
        bold: run_style.bold,
        italic: run_style.italic,
        underline: run_style.underline,
        href,
        protect_kind: None,
        writeback_key,
    })
}

fn is_page_break(event: &BytesStart<'_>) -> bool {
    attr_value(event, b"type").as_deref() == Some("page")
}

fn is_embedded_object_name(name: &[u8]) -> bool {
    matches!(name, b"object" | b"OLEObject" | b"chart" | b"relIds")
}

fn extract_writeback_paragraph_templates(
    xml: &str,
    support: &DocxSupportData,
) -> Result<Vec<WritebackBlockTemplate>, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);

    let mut buf = Vec::new();
    let mut body_depth = 0usize;
    let mut block_depth = 0usize;
    let mut block_name: Option<Vec<u8>> = None;
    let mut block_events: Vec<Event<'static>> = Vec::new();
    let mut blocks = Vec::new();
    let mut numbering_tracker = NumberingTracker::default();

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
                                support,
                                &mut numbering_tracker,
                            )?,
                        )),
                        b"tbl" => blocks.push(parse_table_placeholder_block(&[Event::Empty(e)])?),
                        b"sdt" => blocks.push(parse_sdt_placeholder_block(&[Event::Empty(e)])?),
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
                                    support,
                                    &mut numbering_tracker,
                                )?,
                            ),
                            b"tbl" => parse_table_placeholder_block(&block_events)?,
                            b"sdt" => parse_sdt_placeholder_block(&block_events)?,
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

fn parse_sdt_placeholder_block(
    events: &[Event<'static>],
) -> Result<WritebackBlockTemplate, String> {
    let (text, kind) = classify_block_sdt(events);
    parse_locked_block(events, text, kind)
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
    build_display_block_texts(blocks)
        .join(DOCX_BLOCK_SEPARATOR)
        .trim_matches('\u{feff}')
        .to_string()
}

fn build_display_block_texts(blocks: &[WritebackBlockTemplate]) -> Vec<String> {
    build_display_blocks(blocks)
        .into_iter()
        .map(|display_block| display_block_text(blocks, &display_block))
        .collect()
}

fn display_block_text(
    blocks: &[WritebackBlockTemplate],
    display_block: &DisplayBlockRef,
) -> String {
    match display_block.kind {
        DisplayBlockKind::Paragraph { block_index } => {
            let WritebackBlockTemplate::Paragraph(paragraph) = &blocks[block_index] else {
                return String::new();
            };
            display_block
                .region_indices
                .iter()
                .filter_map(|region_index| paragraph.regions.get(*region_index))
                .map(WritebackRegionTemplate::text)
                .collect()
        }
        DisplayBlockKind::LockedBlock { block_index } => match &blocks[block_index] {
            WritebackBlockTemplate::Locked(region) => region.text.clone(),
            WritebackBlockTemplate::Paragraph(_) => String::new(),
        },
    }
}

#[cfg(test)]
fn flatten_writeback_blocks(
    blocks: &[WritebackBlockTemplate],
    rewrite_headings: bool,
) -> Vec<TextRegion> {
    let display_blocks = build_display_blocks(blocks);
    let mut regions = Vec::new();

    for (display_index, display_block) in display_blocks.iter().enumerate() {
        let append_block_separator = display_index + 1 < display_blocks.len();
        match display_block.kind {
            DisplayBlockKind::Paragraph { block_index } => {
                let WritebackBlockTemplate::Paragraph(paragraph) = &blocks[block_index] else {
                    continue;
                };
                if display_block.region_indices.is_empty() {
                    regions.push(TextRegion::locked_text(if append_block_separator {
                        DOCX_BLOCK_SEPARATOR.to_string()
                    } else {
                        String::new()
                    }));
                    continue;
                }
                for (region_position, region_index) in
                    display_block.region_indices.iter().enumerate()
                {
                    let Some(region) = paragraph.regions.get(*region_index) else {
                        continue;
                    };
                    let mut body = region.text().to_string();
                    if append_block_separator
                        && region_position + 1 == display_block.region_indices.len()
                    {
                        body.push_str(DOCX_BLOCK_SEPARATOR);
                    }
                    let editable =
                        !paragraph_region_skip_rewrite(paragraph, region, rewrite_headings);
                    regions.push(if editable {
                        TextRegion::editable(body).with_presentation(region.presentation().cloned())
                    } else {
                        locked_region_from_presentation(body, region.presentation().cloned())
                    });
                }
            }
            DisplayBlockKind::LockedBlock { block_index } => {
                let WritebackBlockTemplate::Locked(region) = &blocks[block_index] else {
                    continue;
                };
                let mut body = region.text.clone();
                if append_block_separator {
                    body.push_str(DOCX_BLOCK_SEPARATOR);
                }
                regions.push(locked_region_from_presentation(
                    body,
                    region.presentation.clone(),
                ));
            }
        }
    }

    regions
}

#[cfg(test)]
fn paragraph_region_skip_rewrite(
    paragraph: &WritebackParagraphTemplate,
    region: &WritebackRegionTemplate,
    rewrite_headings: bool,
) -> bool {
    if paragraph.is_heading && !rewrite_headings {
        return true;
    }
    region.skip_rewrite()
}

#[cfg(test)]
fn flatten_writeback_blocks_for_test(blocks: &[WritebackBlockTemplate]) -> Vec<TextRegion> {
    flatten_writeback_blocks(blocks, false)
}

fn parse_writeback_paragraph_template(
    events: &[Event<'static>],
    support: &DocxSupportData,
    numbering_tracker: &mut NumberingTracker,
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
    let is_heading =
        paragraph_properties_indicate_heading(&paragraph_property_events, &support.styles);
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
                        &support.hyperlink_targets,
                    )?),
                    b"oMath" | b"oMathPara" => {
                        regions.push(parse_writeback_formula_region(&child_events)?)
                    }
                    name if is_locked_inline_object_name(name) => {
                        regions.push(writeback_locked_region_from_special(&child_events)?)
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

    if writeback_regions_have_visible_content(&regions) {
        if let Some(marker) = list_marker_for_paragraph(
            &support.numbering,
            &support.styles,
            numbering_tracker,
            &paragraph_property_events,
        ) {
            regions.insert(
                0,
                placeholders::synthetic_locked_region(&marker, "list-marker"),
            );
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
                        regions.push(writeback_locked_region_from_special(&child_events)?)
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

fn writeback_regions_have_visible_content(regions: &[WritebackRegionTemplate]) -> bool {
    regions
        .iter()
        .any(|region| text_has_visible_content(region.text()))
}

fn text_has_visible_content(text: &str) -> bool {
    !text.trim().is_empty()
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
    if current.presentation != next.presentation || current.display_mode != next.display_mode {
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
    if current.allow_rewrite != next.allow_rewrite || current.presentation != next.presentation {
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
                    regions.push(writeback_locked_run_special_region(
                        &run_property_events,
                        &child_events,
                    )?);
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
                    regions.push(writeback_locked_run_special_region(
                        &run_property_events,
                        &empty_events,
                    )?);
                    index += 1;
                    continue;
                }
                match name.as_slice() {
                    b"t" | b"rPr" => {}
                    b"tab" => buffer.push('\t'),
                    b"noBreakHyphen" | b"softHyphen" => {
                        if let Some(ch) = special_run_char(e) {
                            buffer.push(ch);
                        }
                    }
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
                            display_mode: LockedDisplayMode::Inline,
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
) -> Option<TextPresentation> {
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
    presentation: Option<TextPresentation>,
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
    let text = std::mem::take(buffer);
    push_writeback_editable_text_regions(regions, text, presentation, render);
}

fn push_writeback_editable_text_regions(
    regions: &mut Vec<WritebackRegionTemplate>,
    text: String,
    presentation: Option<TextPresentation>,
    render: EditableRegionRender,
) {
    for (segment_text, allow_rewrite) in split_editable_text_segments(&text, &presentation) {
        regions.push(WritebackRegionTemplate::Editable(EditableRegionTemplate {
            allow_rewrite,
            text: segment_text.to_string(),
            presentation: presentation.clone(),
            render: render.clone(),
        }));
    }
}

fn split_editable_text_segments<'a>(
    text: &'a str,
    presentation: &Option<TextPresentation>,
) -> Vec<(&'a str, bool)> {
    let mut segments = Vec::new();
    for (segment_text, allow_rewrite) in split_structured_text_segments(text, presentation) {
        if !allow_rewrite {
            segments.push((segment_text, false));
            continue;
        }
        extend_url_locked_segments(&mut segments, segment_text);
    }
    segments
}

fn split_structured_text_segments<'a>(
    text: &'a str,
    presentation: &Option<TextPresentation>,
) -> Vec<(&'a str, bool)> {
    if !presentation.as_ref().is_some_and(|item| item.underline) || !text_has_visible_content(text)
    {
        return vec![(text, text_has_visible_content(text))];
    }
    let Some((content_start, content_end)) = text_content_bounds(text) else {
        return vec![(text, false)];
    };
    if content_start == 0 && content_end == text.len() {
        return vec![(text, true)];
    }
    let mut segments = Vec::with_capacity(3);
    if content_start > 0 {
        segments.push((&text[..content_start], false));
    }
    segments.push((&text[content_start..content_end], true));
    if content_end < text.len() {
        segments.push((&text[content_end..], false));
    }
    segments
}

fn text_content_bounds(text: &str) -> Option<(usize, usize)> {
    let start = text.char_indices().find(|(_, ch)| !ch.is_whitespace())?.0;
    let (end_start, end_ch) = text
        .char_indices()
        .rev()
        .find(|(_, ch)| !ch.is_whitespace())?;
    Some((start, end_start + end_ch.len_utf8()))
}

fn extend_url_locked_segments<'a>(segments: &mut Vec<(&'a str, bool)>, text: &'a str) {
    let spans = bare_url_spans(text);
    if spans.is_empty() {
        segments.push((text, true));
        return;
    }

    let mut cursor = 0usize;
    for (start, end) in spans {
        if cursor < start {
            let prefix = &text[cursor..start];
            segments.push((prefix, text_has_visible_content(prefix)));
        }
        segments.push((&text[start..end], false));
        cursor = end;
    }
    if cursor < text.len() {
        let suffix = &text[cursor..];
        segments.push((suffix, text_has_visible_content(suffix)));
    }
}

fn bare_url_spans(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut index = 0usize;
    while index < text.len() {
        let slice = &text[index..];
        let prefix_len = if slice.starts_with("https://") {
            Some("https://".len())
        } else if slice.starts_with("http://") {
            Some("http://".len())
        } else if slice.starts_with("www.") {
            Some("www.".len())
        } else {
            None
        };
        let Some(prefix_len) = prefix_len else {
            index += text[index..]
                .chars()
                .next()
                .map(|ch| ch.len_utf8())
                .unwrap_or(1);
            continue;
        };
        if !url_start_allowed(text, index) {
            index += prefix_len;
            continue;
        }
        let end = find_bare_url_end(text, index, prefix_len);
        if end > index + prefix_len {
            spans.push((index, end));
            index = end;
        } else {
            index += prefix_len;
        }
    }
    spans
}

fn url_start_allowed(text: &str, start: usize) -> bool {
    let Some(prev) = text[..start].chars().next_back() else {
        return true;
    };
    !(prev.is_ascii_alphanumeric() || matches!(prev, '/' | '.' | '_' | '-' | '@'))
}

fn find_bare_url_end(text: &str, start: usize, prefix_len: usize) -> usize {
    let bytes = text.as_bytes();
    let mut end = start;
    while end < bytes.len() && !bytes[end].is_ascii_whitespace() {
        end += 1;
    }
    while end > start + prefix_len && url_trailing_punctuation(text[..end].chars().next_back()) {
        end -= text[..end]
            .chars()
            .next_back()
            .map(|ch| ch.len_utf8())
            .unwrap_or(1);
    }
    end
}

fn url_trailing_punctuation(ch: Option<char>) -> bool {
    matches!(
        ch,
        Some(
            '.' | ','
                | ';'
                | ':'
                | '!'
                | '?'
                | ')'
                | ']'
                | '}'
                | '"'
                | '\''
                | '。'
                | '，'
                | '；'
                | '：'
                | '！'
                | '？'
                | '）'
                | '】'
                | '」'
                | '』'
                | '、'
        )
    )
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

fn build_editor_writeback_updated_regions(
    blocks: &[WritebackBlockTemplate],
    updated_text: &str,
) -> Result<Vec<TextRegion>, String> {
    validate_editor_writeback_blocks(blocks)?;
    let display_blocks = build_display_blocks(blocks);
    let updated_paragraphs = split_updated_paragraphs(updated_text, display_blocks.len())?;
    let mut regions = Vec::new();

    for (index, display_block) in display_blocks.iter().enumerate() {
        let updated_paragraph = updated_paragraphs
            .get(index)
            .ok_or_else(|| "docx 段落数量与写回内容不一致，无法生成 document.xml。".to_string())?
            .clone();
        let append_block_separator = index + 1 < display_blocks.len();
        let mut block_regions = match display_block.kind {
            DisplayBlockKind::Paragraph { block_index } => {
                let WritebackBlockTemplate::Paragraph(paragraph) = &blocks[block_index] else {
                    continue;
                };
                build_editor_writeback_paragraph_display_regions(
                    paragraph,
                    &display_block.region_indices,
                    &updated_paragraph,
                )?
            }
            DisplayBlockKind::LockedBlock { block_index } => {
                let WritebackBlockTemplate::Locked(region) = &blocks[block_index] else {
                    continue;
                };
                vec![locked_region_from_presentation(
                    updated_paragraph,
                    region.presentation.clone(),
                )]
            }
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

fn validate_editor_writeback_blocks(blocks: &[WritebackBlockTemplate]) -> Result<(), String> {
    for block in blocks {
        if let WritebackBlockTemplate::Paragraph(paragraph) = block {
            validate_editor_writeback_paragraph(paragraph)?;
        }
    }
    Ok(())
}

fn build_editor_writeback_paragraph_display_regions(
    paragraph: &WritebackParagraphTemplate,
    region_indices: &[usize],
    updated_text: &str,
) -> Result<Vec<TextRegion>, String> {
    if region_indices.is_empty() {
        return Ok(vec![TextRegion::editable(updated_text.to_string())]);
    }

    let template = region_indices
        .iter()
        .filter_map(|region_index| paragraph.regions.get(*region_index).cloned())
        .collect::<Vec<_>>();
    build_editor_writeback_paragraph_regions_from_template(&template, updated_text)
}

fn build_editor_writeback_paragraph_regions_from_template(
    template: &[WritebackRegionTemplate],
    updated_text: &str,
) -> Result<Vec<TextRegion>, String> {
    if template.is_empty() {
        return Ok(vec![TextRegion::editable(updated_text.to_string())]);
    }

    let mut matches = Vec::new();
    collect_editor_writeback_paragraph_matches(
        template,
        updated_text,
        0,
        0,
        Vec::new(),
        &mut matches,
        2,
    );

    match matches.len() {
        1 => Ok(matches.remove(0)),
        0 => Err("写回内容越过原有样式或锁定边界，无法安全写回。".to_string()),
        _ => Err("写回内容在原有样式或锁定边界上的定位存在歧义，无法安全写回。".to_string()),
    }
}

fn validate_editor_writeback_paragraph(
    _paragraph: &WritebackParagraphTemplate,
) -> Result<(), String> {
    Ok(())
}

fn collect_editor_writeback_paragraph_matches(
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

    if template[region_index].skip_rewrite() {
        let locked_text = template[region_index].text().to_string();
        let Some(rest) = updated_text.get(cursor..) else {
            return;
        };
        if !rest.starts_with(&locked_text) {
            return;
        }
        let mut next = current;
        next.push(logical_locked_text_region(&template[region_index]));
        collect_editor_writeback_paragraph_matches(
            template,
            updated_text,
            region_index + 1,
            cursor + locked_text.len(),
            next,
            matches,
            limit,
        );
        return;
    }

    collect_editor_writeback_editable_group_matches(
        template,
        updated_text,
        region_index,
        cursor,
        current,
        matches,
        limit,
    );
}

#[derive(Default)]
struct PlainTextEditorEditBlock {
    owner: Option<usize>,
    ambiguous: bool,
    inserted_text: String,
}

fn collect_editor_writeback_editable_group_matches(
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

    if let Some(next_locked) = template.get(group_end) {
        let locked_text = next_locked.text();
        if locked_text.is_empty() {
            return;
        }
        let Some(rest) = updated_text.get(cursor..) else {
            return;
        };
        for (relative_index, _) in rest.match_indices(locked_text) {
            let next_cursor = cursor + relative_index;
            let Some(mapped) =
                map_editable_group_regions(group, &updated_text[cursor..next_cursor])
            else {
                continue;
            };
            let mut next = current.clone();
            next.extend(mapped);
            collect_editor_writeback_paragraph_matches(
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
        return;
    }

    let Some(mapped) = map_editable_group_regions(group, &updated_text[cursor..]) else {
        return;
    };
    let mut completed = current;
    completed.extend(mapped);
    matches.push(completed);
}

fn next_locked_region_index(template: &[WritebackRegionTemplate], start: usize) -> usize {
    let mut index = start;
    while index < template.len() {
        if template[index].skip_rewrite() {
            return index;
        }
        index += 1;
    }
    template.len()
}

fn logical_locked_text_region(region: &WritebackRegionTemplate) -> TextRegion {
    match region {
        WritebackRegionTemplate::Editable(editable) => {
            locked_region_from_presentation(editable.text.clone(), editable.presentation.clone())
        }
        WritebackRegionTemplate::Locked(locked) => {
            locked_region_from_presentation(locked.text.clone(), locked.presentation.clone())
        }
    }
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

    if !flush_editor_writeback_edit_block(&mut outputs, &mut block) {
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
            WritebackRegionTemplate::Editable(region) if region.allow_rewrite => Some(region),
            _ => None,
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
    if !flush_editor_writeback_edit_block(outputs, block) {
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
        add_editor_writeback_block_owner(block, region_index);
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
    add_editor_writeback_block_owner(block, region_index);
    block.inserted_text.push_str(text);
    true
}

fn flush_editor_writeback_edit_block(
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

fn add_editor_writeback_block_owner(block: &mut PlainTextEditorEditBlock, owner: usize) {
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
    if region.allow_rewrite {
        return TextRegion::editable(body).with_presentation(region.presentation.clone());
    }
    locked_region_from_presentation(body, region.presentation.clone())
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
    let display_blocks = build_display_blocks(blocks);
    let mut paragraph_updates = blocks
        .iter()
        .map(|block| match block {
            WritebackBlockTemplate::Paragraph(paragraph) => {
                Some(vec![None::<String>; paragraph.regions.len()])
            }
            WritebackBlockTemplate::Locked(_) => None,
        })
        .collect::<Vec<_>>();
    let mut updated_index = 0usize;
    for (display_index, display_block) in display_blocks.iter().enumerate() {
        let append_block_separator = display_index + 1 < display_blocks.len();
        match display_block.kind {
            DisplayBlockKind::Paragraph { block_index } => {
                let WritebackBlockTemplate::Paragraph(paragraph) = &blocks[block_index] else {
                    continue;
                };
                if display_block.region_indices.is_empty() {
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
                    continue;
                }
                let Some(region_updates) = paragraph_updates
                    .get_mut(block_index)
                    .and_then(Option::as_mut)
                else {
                    return Err("写回内容与原 docx 结构不一致：段落映射丢失。".to_string());
                };
                for (position, region_index) in display_block.region_indices.iter().enumerate() {
                    let updated = updated_regions.get(updated_index).ok_or_else(|| {
                        "写回内容与原 docx 结构不一致：区域数量不足。".to_string()
                    })?;
                    let Some(region) = paragraph.regions.get(*region_index) else {
                        return Err("写回内容与原 docx 结构不一致：段落区域索引越界。".to_string());
                    };
                    let expected_separator = append_block_separator
                        && position + 1 == display_block.region_indices.len();
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
                    region_updates[*region_index] = Some(cleaned);
                    updated_index += 1;
                }
            }
            DisplayBlockKind::LockedBlock { block_index } => {
                let WritebackBlockTemplate::Locked(region) = &blocks[block_index] else {
                    continue;
                };
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

    let mut ordered = Vec::new();
    for updates in paragraph_updates {
        let Some(updates) = updates else {
            continue;
        };
        let mut resolved = Vec::with_capacity(updates.len());
        for value in updates {
            let Some(value) = value else {
                return Err("写回内容与原 docx 结构不一致：段落区域映射不完整。".to_string());
            };
            resolved.push(value);
        }
        ordered.push(resolved);
    }
    Ok(ordered)
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
        LockedRegionRender::RunChildEvents {
            run_property_events,
            child_events,
        } => write_locked_run_child_events(writer, run_property_events, child_events),
        LockedRegionRender::PageBreak => write_locked_page_break(writer),
        LockedRegionRender::Synthetic => Ok(()),
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

fn write_locked_run_child_events(
    writer: &mut Writer<Vec<u8>>,
    run_property_events: &[Event<'static>],
    child_events: &[Event<'static>],
) -> Result<(), String> {
    writer
        .write_event(Event::Start(BytesStart::new("w:r")))
        .map_err(|error| format!("生成 document.xml 失败：{error}"))?;
    for event in run_property_events {
        writer
            .write_event(event.clone())
            .map_err(|error| format!("生成 document.xml 失败：{error}"))?;
    }
    write_raw_locked_events(writer, child_events)?;
    writer
        .write_event(Event::End(BytesEnd::new("w:r")))
        .map_err(|error| format!("生成 document.xml 失败：{error}"))?;
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

fn paragraph_properties_indicate_heading(
    events: &[Event<'static>],
    styles: &ParagraphStyles,
) -> bool {
    events.iter().any(|event| match event {
        Event::Start(e) | Event::Empty(e) if local_name(e.name().as_ref()) == b"outlineLvl" => true,
        Event::Start(e) | Event::Empty(e) if local_name(e.name().as_ref()) == b"pStyle" => {
            attr_value(e, b"val").is_some_and(|value| styles.is_heading(&value))
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
        if ch == '\u{2011}' {
            write_run_text_segment(writer, &mut buffer)?;
            writer
                .write_event(Event::Empty(BytesStart::new("w:noBreakHyphen")))
                .map_err(|error| format!("生成 document.xml 失败：{error}"))?;
            continue;
        }
        if ch == '\u{00ad}' {
            write_run_text_segment(writer, &mut buffer)?;
            writer
                .write_event(Event::Empty(BytesStart::new("w:softHyphen")))
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
