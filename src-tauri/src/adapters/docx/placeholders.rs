use quick_xml::events::Event;

use super::model::{
    LockedDisplayMode, LockedRegionRender, LockedRegionTemplate, WritebackBlockTemplate,
    WritebackRegionTemplate,
};
use crate::models::TextPresentation;

pub(super) const DOCX_IMAGE_PLACEHOLDER: &str = "[图片]";
pub(super) const DOCX_TEXTBOX_PLACEHOLDER: &str = "[文本框]";
pub(super) const DOCX_CHART_PLACEHOLDER: &str = "[图表]";
pub(super) const DOCX_SHAPE_PLACEHOLDER: &str = "[图形]";
pub(super) const DOCX_GROUP_SHAPE_PLACEHOLDER: &str = "[组合图形]";
pub(super) const DOCX_CONTENT_CONTROL_PLACEHOLDER: &str = "[内容控件]";
pub(super) const DOCX_TABLE_PLACEHOLDER: &str = "[表格]";
pub(super) const DOCX_SECTION_BREAK_PLACEHOLDER: &str = "[分节符]";
pub(super) const DOCX_FIELD_PLACEHOLDER: &str = "[字段]";

pub(super) fn placeholder_presentation(kind: &str) -> Option<TextPresentation> {
    Some(TextPresentation {
        bold: false,
        italic: false,
        underline: false,
        href: None,
        protect_kind: Some(kind.to_string()),
        writeback_key: None,
    })
}

pub(super) fn raw_locked_block(
    text: &str,
    kind: &str,
    events: &[Event<'static>],
) -> WritebackBlockTemplate {
    WritebackBlockTemplate::Locked(LockedRegionTemplate {
        text: text.to_string(),
        presentation: placeholder_presentation(kind),
        render: LockedRegionRender::RawEvents(events.to_vec()),
        display_mode: LockedDisplayMode::Inline,
    })
}

pub(super) fn raw_locked_region(
    text: &str,
    kind: &str,
    events: &[Event<'static>],
) -> WritebackRegionTemplate {
    WritebackRegionTemplate::Locked(LockedRegionTemplate {
        text: text.to_string(),
        presentation: placeholder_presentation(kind),
        render: LockedRegionRender::RawEvents(events.to_vec()),
        display_mode: LockedDisplayMode::Inline,
    })
}

pub(super) fn synthetic_locked_region(text: &str, kind: &str) -> WritebackRegionTemplate {
    WritebackRegionTemplate::Locked(LockedRegionTemplate {
        text: text.to_string(),
        presentation: placeholder_presentation(kind),
        render: LockedRegionRender::Synthetic,
        display_mode: LockedDisplayMode::Inline,
    })
}

pub(super) fn raw_locked_region_after_paragraph(
    text: &str,
    kind: &str,
    events: &[Event<'static>],
) -> WritebackRegionTemplate {
    WritebackRegionTemplate::Locked(LockedRegionTemplate {
        text: text.to_string(),
        presentation: placeholder_presentation(kind),
        render: LockedRegionRender::RawEvents(events.to_vec()),
        display_mode: LockedDisplayMode::AfterParagraph,
    })
}

pub(super) fn locked_run_child_region(
    text: &str,
    kind: &str,
    run_property_events: &[Event<'static>],
    child_events: &[Event<'static>],
    display_mode: LockedDisplayMode,
) -> WritebackRegionTemplate {
    WritebackRegionTemplate::Locked(LockedRegionTemplate {
        text: text.to_string(),
        presentation: placeholder_presentation(kind),
        render: LockedRegionRender::RunChildEvents {
            run_property_events: run_property_events.to_vec(),
            child_events: child_events.to_vec(),
        },
        display_mode,
    })
}
