use crate::models::{ChunkPresentation, ChunkPreset};

use super::guards::BoundaryGuard;
use super::masked::{append_segmented_masked_text, append_segmented_text};
use super::{append_separator_to_last, split_trailing_whitespace, SegmentedChunk};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SegmentRegionRole {
    Flow,
    Isolated,
    Separator,
}

#[derive(Debug, Clone)]
pub(crate) struct SegmentRegion {
    pub body: String,
    pub skip_rewrite: bool,
    pub presentation: Option<ChunkPresentation>,
    pub role: SegmentRegionRole,
}

impl SegmentRegion {
    pub(crate) fn flow(
        body: impl Into<String>,
        skip_rewrite: bool,
        presentation: Option<ChunkPresentation>,
    ) -> Self {
        Self {
            body: body.into(),
            skip_rewrite,
            presentation,
            role: SegmentRegionRole::Flow,
        }
    }

    pub(crate) fn isolated(
        body: impl Into<String>,
        skip_rewrite: bool,
        presentation: Option<ChunkPresentation>,
    ) -> Self {
        Self {
            body: body.into(),
            skip_rewrite,
            presentation,
            role: SegmentRegionRole::Isolated,
        }
    }

    pub(crate) fn separator(body: impl Into<String>) -> Self {
        Self {
            body: body.into(),
            skip_rewrite: true,
            presentation: None,
            role: SegmentRegionRole::Separator,
        }
    }
}

pub(crate) fn segment_region_stream<G: BoundaryGuard>(
    regions: Vec<SegmentRegion>,
    preset: ChunkPreset,
) -> Vec<SegmentedChunk> {
    let original = regions
        .iter()
        .map(|region| region.body.as_str())
        .collect::<String>();
    let mut segmenter = RegionStreamSegmenter::<G>::new(preset);
    for region in regions {
        segmenter.push(region);
    }

    let chunks = segmenter.finish();
    if chunks.is_empty() {
        vec![SegmentedChunk {
            text: original,
            separator_after: String::new(),
            skip_rewrite: false,
            presentation: None,
        }]
    } else {
        chunks
    }
}

struct RegionStreamSegmenter<G: BoundaryGuard> {
    chunks: Vec<SegmentedChunk>,
    flow_chars: Vec<char>,
    flow_editable: Vec<bool>,
    preset: ChunkPreset,
    _guard: std::marker::PhantomData<G>,
}

impl<G: BoundaryGuard> RegionStreamSegmenter<G> {
    fn new(preset: ChunkPreset) -> Self {
        Self {
            chunks: Vec::new(),
            flow_chars: Vec::new(),
            flow_editable: Vec::new(),
            preset,
            _guard: std::marker::PhantomData,
        }
    }

    fn push(&mut self, region: SegmentRegion) {
        if region.body.is_empty() {
            return;
        }

        match region.role {
            SegmentRegionRole::Flow => self.push_flow_region(region),
            SegmentRegionRole::Isolated => self.push_isolated_region(region),
            SegmentRegionRole::Separator => self.push_separator_region(region.body),
        }
    }

    fn push_flow_region(&mut self, region: SegmentRegion) {
        for ch in region.body.chars() {
            self.flow_chars.push(ch);
            self.flow_editable.push(!region.skip_rewrite);
        }
    }

    fn push_isolated_region(&mut self, region: SegmentRegion) {
        self.flush_flow();
        if region.skip_rewrite {
            append_raw_chunk(
                &mut self.chunks,
                &region.body,
                true,
                region.presentation.clone(),
            );
            return;
        }
        append_segmented_text::<G>(
            &mut self.chunks,
            &region.body,
            self.preset,
            region.presentation,
        );
    }

    fn push_separator_region(&mut self, body: String) {
        self.flush_flow();
        append_separator_to_last(&mut self.chunks, body);
    }

    fn flush_flow(&mut self) {
        if self.flow_chars.is_empty() {
            return;
        }
        append_segmented_masked_text::<G>(
            &mut self.chunks,
            &self.flow_chars,
            &self.flow_editable,
            self.preset,
            None,
        );
        self.flow_chars.clear();
        self.flow_editable.clear();
    }

    fn finish(mut self) -> Vec<SegmentedChunk> {
        self.flush_flow();
        self.chunks
    }
}

fn append_raw_chunk(
    chunks: &mut Vec<SegmentedChunk>,
    text: &str,
    skip_rewrite: bool,
    presentation: Option<ChunkPresentation>,
) {
    let (body, trailing_ws) = split_trailing_whitespace(text);
    if body.is_empty() {
        append_separator_to_last(chunks, trailing_ws);
        return;
    }

    chunks.push(SegmentedChunk {
        text: body,
        separator_after: trailing_ws,
        skip_rewrite,
        presentation,
    });
}
