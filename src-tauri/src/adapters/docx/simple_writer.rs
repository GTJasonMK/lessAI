use super::*;

pub(super) fn rewrite_document_xml_with_regions(
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

pub(super) fn collect_updated_writeback_regions(
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
                    let sanitized_body =
                        sanitize_updated_region_text(region, updated, &expected_body, paragraph.is_heading)?;
                    let cleaned = if expected_separator {
                        sanitized_body
                            .strip_suffix(DOCX_BLOCK_SEPARATOR)
                            .ok_or_else(|| {
                                "写回内容与原 docx 结构不一致：块分隔符丢失。".to_string()
                            })?
                            .to_string()
                    } else {
                        sanitized_body
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
                sanitize_updated_locked_region_text(region, updated, &expected_body);
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

pub(super) fn validate_updated_empty_paragraph_region(
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

pub(super) fn sanitize_updated_region_text(
    expected: &WritebackRegionTemplate,
    updated: &TextRegion,
    expected_body: &str,
    allow_locked_editable_region: bool,
) -> Result<String, String> {
    if !expected.skip_rewrite()
        && allow_locked_editable_region
        && updated.skip_rewrite
        && expected.presentation() == updated.presentation.as_ref()
    {
        // 对“标题锁定兜底”场景采取保守策略：无论文本是否漂移，都回落到原锁定文本。
        // 这样可以避免误修改标题，同时尽量不中断整篇写回。
        return Ok(expected_body.to_string());
    }
    if expected.skip_rewrite() != updated.skip_rewrite
        || expected.presentation() != updated.presentation.as_ref()
    {
        if expected.skip_rewrite() {
            return Ok(expected_body.to_string());
        }
        return Err(
            "写回内容与原 docx 结构不一致：行内样式、超链接或锁定区边界已变化。".to_string(),
        );
    }
    if expected.skip_rewrite() {
        return Ok(expected_body.to_string());
    }
    Ok(updated.body.clone())
}

pub(super) fn sanitize_updated_locked_region_text(
    expected: &LockedRegionTemplate,
    updated: &TextRegion,
    expected_body: &str,
) {
    if !updated.skip_rewrite
        || updated.presentation.as_ref() != expected.presentation.as_ref()
        || updated.body != expected_body
    {
        // 锁定区只允许原样保留：这里显式吞掉漂移，最终写回仍会使用模板中的原始事件。
    }
}

pub(super) fn write_rewritten_paragraph_from_template(
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

pub(super) fn write_editable_region(
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

pub(super) fn write_locked_region(
    writer: &mut Writer<Vec<u8>>,
    region: &LockedRegionTemplate,
) -> Result<(), String> {
    write_locked_region_render(writer, &region.render)
}

pub(super) fn write_locked_region_render(
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

pub(super) fn write_raw_locked_events(
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

pub(super) fn write_locked_run_child_events(
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

pub(super) fn write_locked_page_break(writer: &mut Writer<Vec<u8>>) -> Result<(), String> {
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

pub(super) fn paragraph_bounds(
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

pub(super) fn collect_paragraph_property_events(events: &[Event<'static>]) -> Vec<Event<'static>> {
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

pub(super) fn paragraph_properties_indicate_heading(
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

pub(super) fn write_styled_text_run(
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

pub(super) fn write_run_text_segment(writer: &mut Writer<Vec<u8>>, buffer: &mut String) -> Result<(), String> {
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

pub(super) fn needs_space_preserve(text: &str) -> bool {
    text.chars().next().is_some_and(char::is_whitespace)
        || text.chars().last().is_some_and(char::is_whitespace)
}

pub(super) fn replace_document_xml(docx_bytes: &[u8], updated_xml: &str) -> Result<Vec<u8>, String> {
    let cursor = Cursor::new(docx_bytes);
    let mut archive = ZipArchive::new(cursor).map_err(format_docx_zip_error)?;
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

pub(super) fn tag_name(name: &[u8]) -> String {
    String::from_utf8_lossy(name).into_owned()
}
