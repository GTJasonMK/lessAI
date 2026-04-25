use super::*;

pub(super) fn is_ignorable_paragraph_name(name: &[u8]) -> bool {
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

pub(super) fn is_ignorable_body_name(name: &[u8]) -> bool {
    is_ignorable_paragraph_name(name)
}

pub(super) fn is_locked_inline_object_name(name: &[u8]) -> bool {
    is_inline_special_name(name)
}

pub(super) fn writeback_locked_region_from_special(
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

pub(super) fn writeback_locked_run_special_region(
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

pub(super) fn special_run_char(event: &BytesStart<'_>) -> Option<char> {
    match local_name(event.name().as_ref()) {
        b"noBreakHyphen" => Some('\u{2011}'),
        b"softHyphen" => Some('\u{00ad}'),
        _ => None,
    }
}


pub(super) fn is_page_break(event: &BytesStart<'_>) -> bool {
    attr_value(event, b"type").as_deref() == Some("page")
}

pub(super) fn is_embedded_object_name(name: &[u8]) -> bool {
    matches!(name, b"object" | b"OLEObject" | b"chart" | b"relIds")
}

