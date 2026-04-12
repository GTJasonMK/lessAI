use quick_xml::events::Event;

use super::model::{
    LockedRegionRender, LockedRegionTemplate, WritebackBlockTemplate, WritebackRegionTemplate,
};
use crate::{adapters::TextRegion, models::ChunkPresentation};

pub(super) const DOCX_IMAGE_PLACEHOLDER: &str = "[图片]";
pub(super) const DOCX_TEXTBOX_PLACEHOLDER: &str = "[文本框]";
pub(super) const DOCX_CHART_PLACEHOLDER: &str = "[图表]";
pub(super) const DOCX_SHAPE_PLACEHOLDER: &str = "[图形]";
pub(super) const DOCX_GROUP_SHAPE_PLACEHOLDER: &str = "[组合图形]";
pub(super) const DOCX_TOC_PLACEHOLDER: &str = "[目录]";
pub(super) const DOCX_TABLE_PLACEHOLDER: &str = "[表格]";
pub(super) const DOCX_SECTION_BREAK_PLACEHOLDER: &str = "[分节符]";

pub(super) fn placeholder_presentation(kind: &str) -> Option<ChunkPresentation> {
    Some(ChunkPresentation {
        bold: false,
        italic: false,
        underline: false,
        href: None,
        protect_kind: Some(kind.to_string()),
        writeback_key: None,
    })
}

pub(super) fn locked_region(text: &str, kind: &str) -> TextRegion {
    TextRegion {
        body: text.to_string(),
        skip_rewrite: true,
        presentation: placeholder_presentation(kind),
    }
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
    })
}
