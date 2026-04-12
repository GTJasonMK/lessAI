use quick_xml::events::{BytesEnd, BytesStart, Event};

use crate::models::ChunkPresentation;

#[derive(Debug, Clone)]
pub(crate) enum WritebackBlockTemplate {
    Paragraph(WritebackParagraphTemplate),
    Locked(LockedRegionTemplate),
}

impl WritebackBlockTemplate {
    pub fn text(&self) -> String {
        match self {
            Self::Paragraph(paragraph) => paragraph.text(),
            Self::Locked(region) => region.text.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct WritebackParagraphTemplate {
    pub paragraph_start: BytesStart<'static>,
    pub paragraph_end: BytesEnd<'static>,
    pub is_heading: bool,
    pub paragraph_property_events: Vec<Event<'static>>,
    pub regions: Vec<WritebackRegionTemplate>,
}

impl WritebackParagraphTemplate {
    pub fn text(&self) -> String {
        self.regions
            .iter()
            .map(WritebackRegionTemplate::text)
            .collect::<String>()
    }
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
        matches!(self, Self::Locked(_))
    }

    pub fn presentation(&self) -> Option<&ChunkPresentation> {
        match self {
            Self::Editable(region) => region.presentation.as_ref(),
            Self::Locked(region) => region.presentation.as_ref(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct EditableRegionTemplate {
    pub text: String,
    pub presentation: Option<ChunkPresentation>,
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
    pub presentation: Option<ChunkPresentation>,
    pub render: LockedRegionRender,
}

#[derive(Debug, Clone)]
pub(crate) enum LockedRegionRender {
    RawEvents(Vec<Event<'static>>),
    PageBreak,
    Sequence(Vec<LockedRegionRender>),
}
