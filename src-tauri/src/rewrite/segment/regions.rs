use crate::adapters::TextRegion;
use crate::documents::RegionSegmentationStrategy;
use crate::models::{ChunkPreset, DocumentFormat};

use super::guards::{NoopBoundaryGuard, TexBraceBoundaryGuard};
use super::postprocess::merge_left_binding_punctuation_chunks;
use super::stream::{segment_region_stream, SegmentRegion};
use super::SegmentedChunk;

fn segment_preserved_regions(regions: Vec<TextRegion>, preset: ChunkPreset) -> Vec<SegmentedChunk> {
    let stream = regions
        .into_iter()
        .filter(|region| !region.body.is_empty())
        .map(|region| {
            SegmentRegion::isolated(region.body, region.skip_rewrite, region.presentation)
        })
        .collect::<Vec<_>>();
    segment_region_stream::<NoopBoundaryGuard>(stream, preset)
}

pub fn segment_regions_with_strategy(
    regions: Vec<TextRegion>,
    preset: ChunkPreset,
    format: DocumentFormat,
    strategy: RegionSegmentationStrategy,
) -> Vec<SegmentedChunk> {
    let chunks = match strategy {
        RegionSegmentationStrategy::PreserveBoundaries => {
            segment_preserved_regions(regions, preset)
        }
        RegionSegmentationStrategy::FormatAware => segment_text_regions(regions, preset, format),
    };
    merge_left_binding_punctuation_chunks(chunks)
}

fn segment_text_regions(
    regions: Vec<TextRegion>,
    preset: ChunkPreset,
    format: DocumentFormat,
) -> Vec<SegmentedChunk> {
    let stream = match format {
        DocumentFormat::PlainText => regions
            .into_iter()
            .filter(|region| !region.body.is_empty())
            .map(|region| {
                SegmentRegion::flow(region.body, region.skip_rewrite, region.presentation)
            })
            .collect::<Vec<_>>(),
        DocumentFormat::Markdown => super::markdown::build_markdown_stream(regions),
        DocumentFormat::Tex => super::tex::build_tex_stream(regions),
    };

    match format {
        DocumentFormat::Tex => segment_region_stream::<TexBraceBoundaryGuard>(stream, preset),
        DocumentFormat::PlainText | DocumentFormat::Markdown => {
            segment_region_stream::<NoopBoundaryGuard>(stream, preset)
        }
    }
}
