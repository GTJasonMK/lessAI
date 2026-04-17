use quick_xml::events::{BytesEnd, BytesStart, Event};

use crate::models::TextPresentation;

#[derive(Debug, Clone)]
pub(crate) enum WritebackBlockTemplate {
    Paragraph(WritebackParagraphTemplate),
    Locked(LockedRegionTemplate),
}

#[derive(Debug, Clone)]
pub(crate) struct WritebackParagraphTemplate {
    pub paragraph_start: BytesStart<'static>,
    pub paragraph_end: BytesEnd<'static>,
    pub is_heading: bool,
    pub paragraph_property_events: Vec<Event<'static>>,
    pub regions: Vec<WritebackRegionTemplate>,
}

#[derive(Debug, Clone)]
pub(crate) enum WritebackRegionTemplate {
    Editable(EditableRegionTemplate),
    Locked(LockedRegionTemplate),
}

impl WritebackRegionTemplate {
    pub fn text(&self) -> &str {
        match self {
            Self::Editable(region) => &region.text,
            Self::Locked(region) => &region.text,
        }
    }

    pub fn skip_rewrite(&self) -> bool {
        match self {
            Self::Editable(region) => !region.allow_rewrite,
            Self::Locked(_) => true,
        }
    }

    pub fn presentation(&self) -> Option<&TextPresentation> {
        match self {
            Self::Editable(region) => region.presentation.as_ref(),
            Self::Locked(region) => region.presentation.as_ref(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct EditableRegionTemplate {
    pub text: String,
    pub allow_rewrite: bool,
    pub presentation: Option<TextPresentation>,
    pub render: EditableRegionRender,
}

#[derive(Debug, Clone)]
pub(crate) enum EditableRegionRender {
    Run {
        run_property_events: Vec<Event<'static>>,
    },
    Hyperlink {
        hyperlink_start: BytesStart<'static>,
        run_property_events: Vec<Event<'static>>,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct LockedRegionTemplate {
    pub text: String,
    pub presentation: Option<TextPresentation>,
    pub render: LockedRegionRender,
    pub display_mode: LockedDisplayMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LockedDisplayMode {
    Inline,
    AfterParagraph,
}

#[derive(Debug, Clone)]
pub(crate) enum LockedRegionRender {
    RawEvents(Vec<Event<'static>>),
    RunChildEvents {
        run_property_events: Vec<Event<'static>>,
        child_events: Vec<Event<'static>>,
    },
    PageBreak,
    Synthetic,
    Sequence(Vec<LockedRegionRender>),
}
