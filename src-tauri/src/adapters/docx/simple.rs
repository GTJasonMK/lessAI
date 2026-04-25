use std::{
    collections::HashMap,
    io::{Cursor, Read, Write},
};

use quick_xml::{
    events::{BytesEnd, BytesStart, BytesText, Event},
    Reader, Writer,
};
use zip::{write::FileOptions, ZipArchive, ZipWriter};

use super::{
    display::{build_display_blocks, DisplayBlockKind, DisplayBlockRef},
    model::{
        EditableRegionRender, EditableRegionTemplate, LockedDisplayMode, LockedRegionRender,
        LockedRegionTemplate, WritebackBlockTemplate, WritebackParagraphTemplate,
        WritebackRegionTemplate,
    },
    numbering::{list_marker_for_paragraph, NumberingTracker},
    package::{format_docx_zip_error, load_docx_document, DocxSupportData},
    placeholders,
    signature::build_docx_writeback_model,
    specials::{classify_block_sdt, classify_inline_special_region, is_inline_special_name},
    styles::ParagraphStyles,
    xml::{
        attr_value, hyperlink_target, local_name, local_name_owned, toggle_attr_enabled,
        underline_enabled,
    },
};
use crate::{
    adapters::TextRegion,
    models::{DiffType, TextPresentation},
    rewrite_unit::WritebackSlot,
};

/// Docx 适配器：从 `.docx`（Office Open XML）中抽取可改写的纯文本。
///
/// 重要说明：
/// - `.docx` 是 zip + XML 的二进制容器；
/// - 对常见复杂结构会以“锁定占位符”导入并原样保留；
/// - 对无法安全兜底的结构（如嵌入 Office 对象）仍会直接报错，避免不确定写回。
pub struct DocxAdapter;

impl DocxAdapter {
    pub(crate) fn load_writeback_source(
        docx_bytes: &[u8],
    ) -> Result<LoadedDocxWritebackSource, String> {
        load_docx_writeback_source(docx_bytes)
    }

    pub(crate) fn extract_writeback_model_from_source(
        loaded: &LoadedDocxWritebackSource,
        rewrite_headings: bool,
    ) -> super::signature::DocxWritebackModel {
        build_docx_writeback_model(&loaded.blocks, rewrite_headings)
    }

    #[cfg(test)]
    pub fn extract_text(docx_bytes: &[u8]) -> Result<String, String> {
        let loaded = load_docx_document(docx_bytes)?;
        let regions =
            extract_regions_from_document_xml(&loaded.document_xml, &loaded.support, true)?;
        Ok(regions
            .into_iter()
            .map(|region| region.body)
            .collect::<String>()
            .trim_matches('\u{feff}')
            .to_string())
    }

    #[cfg(test)]
    pub(crate) fn extract_writeback_source_text(docx_bytes: &[u8]) -> Result<String, String> {
        let loaded = load_docx_document(docx_bytes)?;
        let blocks = extract_writeback_paragraph_templates(&loaded.document_xml, &loaded.support)?;
        Ok(build_writeback_source_text(&blocks))
    }

    #[cfg(test)]
    pub fn extract_writeback_regions(docx_bytes: &[u8]) -> Result<Vec<TextRegion>, String> {
        let loaded = load_docx_document(docx_bytes)?;
        let blocks = extract_writeback_paragraph_templates(&loaded.document_xml, &loaded.support)?;
        Ok(flatten_writeback_blocks_for_test(&blocks))
    }

    #[cfg(test)]
    pub fn extract_regions(
        docx_bytes: &[u8],
        rewrite_headings: bool,
    ) -> Result<Vec<TextRegion>, String> {
        let loaded = load_docx_document(docx_bytes)?;
        extract_regions_from_document_xml(&loaded.document_xml, &loaded.support, rewrite_headings)
    }

    #[cfg(test)]
    pub fn extract_writeback_slots(
        docx_bytes: &[u8],
        rewrite_headings: bool,
    ) -> Result<Vec<crate::rewrite_unit::WritebackSlot>, String> {
        Ok(Self::extract_writeback_model(docx_bytes, rewrite_headings)?.writeback_slots)
    }

    #[cfg(test)]
    pub fn extract_writeback_model(
        docx_bytes: &[u8],
        rewrite_headings: bool,
    ) -> Result<super::signature::DocxWritebackModel, String> {
        let loaded = Self::load_writeback_source(docx_bytes)?;
        Ok(Self::extract_writeback_model_from_source(
            &loaded,
            rewrite_headings,
        ))
    }

    #[cfg(test)]
    pub fn write_updated_text(
        docx_bytes: &[u8],
        expected_source_text: &str,
        updated_text: &str,
    ) -> Result<Vec<u8>, String> {
        let loaded = Self::load_writeback_source(docx_bytes)?;
        Self::write_updated_text_with_source(
            docx_bytes,
            &loaded,
            expected_source_text,
            updated_text,
        )
    }

    #[cfg(test)]
    pub fn write_updated_regions(
        docx_bytes: &[u8],
        expected_source_text: &str,
        updated_regions: &[TextRegion],
    ) -> Result<Vec<u8>, String> {
        let loaded = Self::load_writeback_source(docx_bytes)?;
        write_docx_with_regions(docx_bytes, &loaded, expected_source_text, updated_regions)
    }

    #[cfg(test)]
    pub fn write_updated_slots(
        docx_bytes: &[u8],
        expected_source_text: &str,
        updated_slots: &[WritebackSlot],
    ) -> Result<Vec<u8>, String> {
        let loaded = Self::load_writeback_source(docx_bytes)?;
        Self::write_updated_slots_with_source(
            docx_bytes,
            &loaded,
            expected_source_text,
            updated_slots,
        )
    }

    pub(crate) fn write_updated_text_with_source(
        docx_bytes: &[u8],
        loaded: &LoadedDocxWritebackSource,
        expected_source_text: &str,
        updated_text: &str,
    ) -> Result<Vec<u8>, String> {
        let updated_regions = build_editor_writeback_updated_regions(&loaded.blocks, updated_text)?;
        write_docx_with_regions(docx_bytes, loaded, expected_source_text, &updated_regions)
    }

    pub(crate) fn write_updated_slots_with_source(
        docx_bytes: &[u8],
        loaded: &LoadedDocxWritebackSource,
        expected_source_text: &str,
        updated_slots: &[WritebackSlot],
    ) -> Result<Vec<u8>, String> {
        let updated_regions = text_regions_from_writeback_slots(updated_slots);
        write_docx_with_regions(docx_bytes, loaded, expected_source_text, &updated_regions)
    }

    #[cfg(test)]
    pub fn validate_writeback(docx_bytes: &[u8]) -> Result<(), String> {
        Self::load_writeback_source(docx_bytes).map(|_| ())
    }

    #[cfg(test)]
    pub fn validate_editor_writeback(docx_bytes: &[u8]) -> Result<(), String> {
        Self::validate_writeback(docx_bytes)
    }
}

const DOCX_BLOCK_SEPARATOR: &str = "\n\n";
const DOCX_HYPERLINK_LOCK_FALLBACK_SIGNAL: &str = "__LESSAI_LOCK_WHOLE_HYPERLINK__";

pub(crate) struct LoadedDocxWritebackSource {
    document_xml: String,
    blocks: Vec<WritebackBlockTemplate>,
}


mod simple_source;
mod simple_utils;
mod simple_signature;
mod simple_extract;
mod simple_merge;
mod simple_extract_runs;
mod simple_editor_writeback;
mod simple_writer;

use self::simple_source::*;
use self::simple_utils::*;
use self::simple_signature::*;
use self::simple_extract::*;
use self::simple_merge::*;
use self::simple_extract_runs::*;
use self::simple_editor_writeback::*;
use self::simple_writer::*;
