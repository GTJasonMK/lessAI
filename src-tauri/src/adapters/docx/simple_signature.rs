use super::*;

pub(super) fn update_run_style(current_run_style: &mut RunStyle, event: &BytesStart<'_>) {
    match local_name(event.name().as_ref()) {
        b"b" => current_run_style.bold = toggle_attr_enabled(event),
        b"i" => current_run_style.italic = toggle_attr_enabled(event),
        b"u" => current_run_style.underline = underline_enabled(event),
        _ => {}
    }
}

pub(super) fn append_start_signature(signature: &mut String, prefix: char, event: &BytesStart<'_>) {
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

pub(super) fn should_ignore_signature_attr(event_name: &[u8], attr_key: &[u8]) -> bool {
    event_name == b"rFonts" && local_name(attr_key) == b"hint"
}

pub(super) fn append_end_signature(signature: &mut String, event: &BytesEnd<'_>) {
    signature.push('E');
    signature.push_str(std::str::from_utf8(event.name().as_ref()).unwrap_or("?"));
    signature.push(';');
}

pub(super) fn append_text_signature(signature: &mut String, prefix: char, text: &str) {
    signature.push(prefix);
    signature.push_str(text);
    signature.push(';');
}

pub(super) fn append_non_whitespace_signature(signature: &mut String, prefix: char, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    append_text_signature(signature, prefix, text);
}

pub(super) fn bytes_start_signature(event: &BytesStart<'_>) -> String {
    let mut signature = String::new();
    append_start_signature(&mut signature, 'S', event);
    signature
}

pub(super) fn events_signature(events: &[Event<'static>]) -> String {
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

pub(super) fn run_property_signature(events: &[Event<'static>]) -> String {
    events_signature(&normalize_run_property_events(events))
}

pub(super) fn normalize_run_property_events(events: &[Event<'static>]) -> Vec<Event<'static>> {
    let filtered = events
        .iter()
        .filter(|event| !should_drop_run_property_event(event))
        .cloned()
        .collect::<Vec<_>>();

    strip_empty_run_property_wrapper(filtered)
}

pub(super) fn should_drop_run_property_event(event: &Event<'static>) -> bool {
    match event {
        Event::Empty(e) => should_drop_empty_run_property_event(e),
        _ => false,
    }
}

pub(super) fn should_drop_empty_run_property_event(event: &BytesStart<'_>) -> bool {
    let name_binding = event.name();
    let name = local_name(name_binding.as_ref());
    matches!(name, b"rPr" | b"rFonts") && !has_meaningful_signature_attr(name, event)
}

pub(super) fn has_meaningful_signature_attr(event_name: &[u8], event: &BytesStart<'_>) -> bool {
    event
        .attributes()
        .flatten()
        .any(|attr| !should_ignore_signature_attr(event_name, attr.key.as_ref()))
}

pub(super) fn strip_empty_run_property_wrapper(events: Vec<Event<'static>>) -> Vec<Event<'static>> {
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

pub(super) fn build_editable_writeback_key(
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

pub(super) fn current_run_writeback_key(
    run_property_events: &[Event<'static>],
    hyperlink_signature: Option<&str>,
) -> Option<String> {
    build_editable_writeback_key(
        &run_property_signature(run_property_events),
        hyperlink_signature,
    )
}

pub(super) fn current_editable_presentation(
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

