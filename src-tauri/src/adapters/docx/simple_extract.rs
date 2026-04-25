use super::*;

pub(super) fn extract_writeback_paragraph_templates(
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
                            name if is_ignorable_body_name(name) => {
                                body_depth += 1;
                            }
                            _ => {
                                block_depth = 1;
                                block_name = Some(name.clone());
                                block_events.clear();
                                block_events.push(Event::Start(e));
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
                        name if is_ignorable_body_name(name) => {}
                        _ => blocks.push(parse_unknown_block_placeholder(
                            &[Event::Empty(e)],
                            name.as_slice(),
                        )?),
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
                            _ => parse_unknown_block_placeholder(&block_events, kind.as_slice())?,
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

pub(super) fn parse_table_placeholder_block(
    events: &[Event<'static>],
) -> Result<WritebackBlockTemplate, String> {
    parse_locked_block(events, placeholders::DOCX_TABLE_PLACEHOLDER, "table")
}

pub(super) fn parse_section_break_placeholder_block(
    events: &[Event<'static>],
) -> Result<WritebackBlockTemplate, String> {
    parse_locked_block(
        events,
        placeholders::DOCX_SECTION_BREAK_PLACEHOLDER,
        "section-break",
    )
}

pub(super) fn parse_sdt_placeholder_block(
    events: &[Event<'static>],
) -> Result<WritebackBlockTemplate, String> {
    let (text, kind) = classify_block_sdt(events);
    parse_locked_block(events, text, kind)
}

pub(super) fn parse_locked_block(
    events: &[Event<'static>],
    text: &str,
    protect_kind: &str,
) -> Result<WritebackBlockTemplate, String> {
    if events.is_empty() {
        return Err("解析 docx 锁定块失败：事件为空。".to_string());
    }
    Ok(placeholders::raw_locked_block(text, protect_kind, events))
}

pub(super) fn unknown_structure_placeholder(name: &[u8]) -> String {
    format!("[复杂结构:{}]", tag_name(name))
}

pub(super) fn parse_unknown_block_placeholder(
    events: &[Event<'static>],
    name: &[u8],
) -> Result<WritebackBlockTemplate, String> {
    let text = unknown_structure_placeholder(name);
    parse_locked_block(events, &text, "unknown-structure")
}

pub(super) fn parse_unknown_inline_placeholder_region(
    events: &[Event<'static>],
    name: &[u8],
) -> WritebackRegionTemplate {
    let text = unknown_structure_placeholder(name);
    placeholders::raw_locked_region(&text, "unknown-structure", events)
}

pub(super) fn parse_unknown_run_placeholder_region(
    run_property_events: &[Event<'static>],
    child_events: &[Event<'static>],
    name: &[u8],
) -> WritebackRegionTemplate {
    let text = unknown_structure_placeholder(name);
    placeholders::locked_run_child_region(
        &text,
        "unknown-structure",
        run_property_events,
        child_events,
        LockedDisplayMode::Inline,
    )
}

pub(super) fn parse_locked_visible_text_region(
    text: &str,
    events: &[Event<'static>],
) -> WritebackRegionTemplate {
    WritebackRegionTemplate::Locked(LockedRegionTemplate {
        text: text.to_string(),
        presentation: None,
        render: LockedRegionRender::RawEvents(events.to_vec()),
        display_mode: LockedDisplayMode::Inline,
    })
}

pub(super) fn parse_locked_visible_run_text_region(
    text: &str,
    presentation: Option<TextPresentation>,
    run_property_events: &[Event<'static>],
    child_events: &[Event<'static>],
) -> WritebackRegionTemplate {
    WritebackRegionTemplate::Locked(LockedRegionTemplate {
        text: text.to_string(),
        presentation,
        render: LockedRegionRender::RunChildEvents {
            run_property_events: run_property_events.to_vec(),
            child_events: child_events.to_vec(),
        },
        display_mode: LockedDisplayMode::Inline,
    })
}

pub(super) fn lock_whole_hyperlink_region(events: &[Event<'static>]) -> WritebackRegionTemplate {
    parse_unknown_inline_placeholder_region(events, b"hyperlink")
}

pub(super) fn next_index_after_ignorable_writeback_event(
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

pub(super) fn trim_writeback_block_bom(blocks: &mut [WritebackBlockTemplate]) {
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

pub(super) fn trim_region_start_bom(region: &mut WritebackRegionTemplate) {
    match region {
        WritebackRegionTemplate::Editable(editable) => {
            editable.text = editable.text.trim_start_matches('\u{feff}').to_string();
        }
        WritebackRegionTemplate::Locked(locked) => {
            locked.text = locked.text.trim_start_matches('\u{feff}').to_string();
        }
    }
}

pub(super) fn trim_region_end_bom(region: &mut WritebackRegionTemplate) {
    match region {
        WritebackRegionTemplate::Editable(editable) => {
            editable.text = editable.text.trim_end_matches('\u{feff}').to_string();
        }
        WritebackRegionTemplate::Locked(locked) => {
            locked.text = locked.text.trim_end_matches('\u{feff}').to_string();
        }
    }
}

pub(super) fn build_writeback_source_text(blocks: &[WritebackBlockTemplate]) -> String {
    build_display_block_texts(blocks)
        .join(DOCX_BLOCK_SEPARATOR)
        .trim_matches('\u{feff}')
        .to_string()
}

pub(super) fn build_display_block_texts(blocks: &[WritebackBlockTemplate]) -> Vec<String> {
    build_display_blocks(blocks)
        .into_iter()
        .map(|display_block| display_block_text(blocks, &display_block))
        .collect()
}

pub(super) fn display_block_text(
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
pub(super) fn flatten_writeback_blocks(
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
pub(super) fn paragraph_region_skip_rewrite(
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
pub(super) fn flatten_writeback_blocks_for_test(blocks: &[WritebackBlockTemplate]) -> Vec<TextRegion> {
    flatten_writeback_blocks(blocks, false)
}

pub(super) fn parse_writeback_paragraph_template(
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
                        regions.push(parse_unknown_inline_placeholder_region(&child_events, name))
                    }
                    _ => regions.push(parse_unknown_inline_placeholder_region(
                        &child_events,
                        name.as_slice(),
                    )),
                }
                index = next_index;
            }
            Event::Text(e) => {
                let decoded = e
                    .decode()
                    .map_err(|error| format!("解析 document.xml 文本失败：{error}"))?;
                if !decoded.trim().is_empty() {
                    let raw_events = [events[index].clone()];
                    regions.push(parse_locked_visible_text_region(
                        decoded.as_ref(),
                        &raw_events,
                    ));
                }
                index += 1;
            }
            Event::CData(e) => {
                let decoded = e
                    .decode()
                    .map_err(|error| format!("解析 document.xml CDATA 失败：{error}"))?;
                if !decoded.trim().is_empty() {
                    let raw_events = [events[index].clone()];
                    regions.push(parse_locked_visible_text_region(
                        decoded.as_ref(),
                        &raw_events,
                    ));
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

pub(super) fn skip_subtree_events(events: &[Event<'static>], start_index: usize) -> Result<usize, String> {
    let (_, next_index) = capture_subtree_events(events, start_index)?;
    Ok(next_index)
}

pub(super) fn capture_subtree_events(
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

pub(super) fn parse_writeback_hyperlink_regions(
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
                    b"r" => {
                        let run_regions = match parse_writeback_run_regions(
                            &child_events,
                            Some(&hyperlink_start),
                            hyperlink_href.clone(),
                        ) {
                            Ok(run_regions) => run_regions,
                            Err(error) if error == DOCX_HYPERLINK_LOCK_FALLBACK_SIGNAL => {
                                return Ok(vec![lock_whole_hyperlink_region(events)]);
                            }
                            Err(error) => return Err(error),
                        };
                        regions.extend(run_regions);
                    }
                    _ => return Ok(vec![lock_whole_hyperlink_region(events)]),
                }
                index = next_index;
            }
            Event::Text(e) => {
                let decoded = e
                    .decode()
                    .map_err(|error| format!("解析 document.xml 文本失败：{error}"))?;
                if !decoded.trim().is_empty() {
                    return Ok(vec![lock_whole_hyperlink_region(events)]);
                }
                index += 1;
            }
            Event::CData(e) => {
                let decoded = e
                    .decode()
                    .map_err(|error| format!("解析 document.xml CDATA 失败：{error}"))?;
                if !decoded.trim().is_empty() {
                    return Ok(vec![lock_whole_hyperlink_region(events)]);
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

