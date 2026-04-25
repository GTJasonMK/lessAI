use super::*;

pub(super) fn parse_writeback_run_regions(
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
                    if hyperlink_start.is_some() {
                        return Err(DOCX_HYPERLINK_LOCK_FALLBACK_SIGNAL.to_string());
                    }
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
                        if hyperlink_start.is_some() {
                            return Err(DOCX_HYPERLINK_LOCK_FALLBACK_SIGNAL.to_string());
                        }
                        let (child_events, next_index) = capture_subtree_events(events, index)?;
                        flush_writeback_editable_region(
                            &mut regions,
                            &mut buffer,
                            presentation.clone(),
                            hyperlink_start.cloned(),
                            &run_property_events,
                        );
                        regions.push(parse_unknown_run_placeholder_region(
                            &run_property_events,
                            &child_events,
                            name,
                        ));
                        index = next_index;
                    }
                    _ => {
                        if hyperlink_start.is_some() {
                            return Err(DOCX_HYPERLINK_LOCK_FALLBACK_SIGNAL.to_string());
                        }
                        let (child_events, next_index) = capture_subtree_events(events, index)?;
                        flush_writeback_editable_region(
                            &mut regions,
                            &mut buffer,
                            presentation.clone(),
                            hyperlink_start.cloned(),
                            &run_property_events,
                        );
                        regions.push(parse_unknown_run_placeholder_region(
                            &run_property_events,
                            &child_events,
                            name.as_slice(),
                        ));
                        index = next_index;
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
                    if hyperlink_start.is_some() {
                        return Err(DOCX_HYPERLINK_LOCK_FALLBACK_SIGNAL.to_string());
                    }
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
                        if hyperlink_start.is_some() {
                            return Err(DOCX_HYPERLINK_LOCK_FALLBACK_SIGNAL.to_string());
                        }
                        flush_writeback_editable_region(
                            &mut regions,
                            &mut buffer,
                            presentation.clone(),
                            hyperlink_start.cloned(),
                            &run_property_events,
                        );
                        regions.push(WritebackRegionTemplate::Locked(LockedRegionTemplate {
                            text: placeholders::DOCX_PAGE_BREAK_PLACEHOLDER.to_string(),
                            presentation: placeholders::placeholder_presentation("page-break"),
                            render: LockedRegionRender::PageBreak,
                            display_mode: LockedDisplayMode::Inline,
                        }));
                    }
                    b"br" | b"cr" => buffer.push('\n'),
                    name if is_embedded_object_name(name) => {
                        if hyperlink_start.is_some() {
                            return Err(DOCX_HYPERLINK_LOCK_FALLBACK_SIGNAL.to_string());
                        }
                        flush_writeback_editable_region(
                            &mut regions,
                            &mut buffer,
                            presentation.clone(),
                            hyperlink_start.cloned(),
                            &run_property_events,
                        );
                        let empty_events = [Event::Empty(e.clone())];
                        regions.push(parse_unknown_run_placeholder_region(
                            &run_property_events,
                            &empty_events,
                            name,
                        ));
                    }
                    _ => {
                        if hyperlink_start.is_some() {
                            return Err(DOCX_HYPERLINK_LOCK_FALLBACK_SIGNAL.to_string());
                        }
                        flush_writeback_editable_region(
                            &mut regions,
                            &mut buffer,
                            presentation.clone(),
                            hyperlink_start.cloned(),
                            &run_property_events,
                        );
                        let empty_events = [Event::Empty(e.clone())];
                        regions.push(parse_unknown_run_placeholder_region(
                            &run_property_events,
                            &empty_events,
                            name.as_slice(),
                        ));
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
                rpr_depth = rpr_depth.saturating_sub(1);
                index += 1;
            }
            Event::Text(e) => {
                let decoded = e
                    .decode()
                    .map_err(|error| format!("解析 document.xml 文本失败：{error}"))?;
                if in_text {
                    buffer.push_str(&decoded);
                } else if !decoded.trim().is_empty() {
                    if hyperlink_start.is_some() {
                        return Err(DOCX_HYPERLINK_LOCK_FALLBACK_SIGNAL.to_string());
                    }
                    flush_writeback_editable_region(
                        &mut regions,
                        &mut buffer,
                        presentation.clone(),
                        hyperlink_start.cloned(),
                        &run_property_events,
                    );
                    let raw_events = [events[index].clone()];
                    regions.push(parse_locked_visible_run_text_region(
                        decoded.as_ref(),
                        presentation.clone(),
                        &run_property_events,
                        &raw_events,
                    ));
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
                    if hyperlink_start.is_some() {
                        return Err(DOCX_HYPERLINK_LOCK_FALLBACK_SIGNAL.to_string());
                    }
                    flush_writeback_editable_region(
                        &mut regions,
                        &mut buffer,
                        presentation.clone(),
                        hyperlink_start.cloned(),
                        &run_property_events,
                    );
                    let raw_events = [events[index].clone()];
                    regions.push(parse_locked_visible_run_text_region(
                        decoded.as_ref(),
                        presentation.clone(),
                        &run_property_events,
                        &raw_events,
                    ));
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

pub(super) fn collect_run_property_events(events: &[Event<'static>]) -> Vec<Event<'static>> {
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

pub(super) fn build_run_presentation(
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

pub(super) fn flush_writeback_editable_region(
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

pub(super) fn push_writeback_editable_text_regions(
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

pub(super) fn split_editable_text_segments<'a>(
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

pub(super) fn split_structured_text_segments<'a>(
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

pub(super) fn text_content_bounds(text: &str) -> Option<(usize, usize)> {
    let start = text.char_indices().find(|(_, ch)| !ch.is_whitespace())?.0;
    let (end_start, end_ch) = text
        .char_indices()
        .rev()
        .find(|(_, ch)| !ch.is_whitespace())?;
    Some((start, end_start + end_ch.len_utf8()))
}

pub(super) fn extend_url_locked_segments<'a>(segments: &mut Vec<(&'a str, bool)>, text: &'a str) {
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

pub(super) fn bare_url_spans(text: &str) -> Vec<(usize, usize)> {
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

pub(super) fn url_start_allowed(text: &str, start: usize) -> bool {
    let Some(prev) = text[..start].chars().next_back() else {
        return true;
    };
    !(prev.is_ascii_alphanumeric() || matches!(prev, '/' | '.' | '_' | '-' | '@'))
}

pub(super) fn find_bare_url_end(text: &str, start: usize, prefix_len: usize) -> usize {
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

pub(super) fn url_trailing_punctuation(ch: Option<char>) -> bool {
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

pub(super) fn parse_writeback_formula_region(
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

