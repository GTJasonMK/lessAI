use std::collections::{HashMap, HashSet};

use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::{
    models::TextPresentation,
    rewrite,
    rewrite_unit::{WritebackSlot, WritebackSlotRole},
    text_boundaries::{
        split_text_and_trailing_separator, split_text_chunks_by_paragraph_separator,
    },
    textual_template::{
        self,
        models::{TextRegionSplitMode, TextTemplateBlock, TextTemplateRegion},
        TextTemplate,
    },
};

const PDF_EMPTY_ERROR: &str = "pdf 文件为空。";
const PDF_NO_VISIBLE_TEXT_ERROR: &str =
    "未从 PDF 中抽取到可见文本。该 PDF 可能是扫描件/图片，需要先做 OCR。";
const PDF_SAFE_WRITEBACK_UNSUPPORTED_PREFIX: &str =
    "当前 PDF 的文本层结构不足以安全进入原文件改写链路；不允许继续 AI 改写或写回原文件。";
const PDF_DUPLICATE_CHUNK_REASON: &str = "同一页存在重复文本块，无法精确定位替换边界。";
const PDF_NO_EDITABLE_CHUNK_REASON: &str = "未抽取到可写回的文本层块。";
const PDF_TOUNICODE_MISSING_REASON: &str = "部分字体缺少 ToUnicode 映射，无法稳定解码文本。";
const PDF_RELOAD_MISMATCH_ERROR: &str =
    "PDF 写回后的文本与预期不一致。当前 PDF 可能使用了受限字体编码，无法稳定写入这些字符。";
const PDF_LINK_PLACEHOLDER: &str = "[链接]";
const PDF_IMAGE_PLACEHOLDER: &str = "[图片]";
const PDF_OBJECT_PLACEHOLDER: &str = "[对象]";
const PDF_GRAPHICS_PLACEHOLDER: &str = "[图形]";
const PDF_LINK_PROTECT_KIND: &str = "pdf-link";
const PDF_IMAGE_PROTECT_KIND: &str = "pdf-image";
const PDF_OBJECT_PROTECT_KIND: &str = "pdf-object";
const PDF_GRAPHICS_PROTECT_KIND: &str = "pdf-graphics";

/// PDF 适配器：优先走可验证的文本层写回子集；不满足安全条件时仅允许导入查看，不进入改写写回链路。
pub struct PdfAdapter;

#[derive(Debug, Clone)]
pub(crate) struct LoadedPdfWritebackSource {
    pub(crate) source_text: String,
    pub(crate) template: TextTemplate,
    pub(crate) writeback_block_reason: Option<String>,
    chunk_entries: Vec<PdfChunkEntry>,
}

impl LoadedPdfWritebackSource {
    pub(crate) fn supports_safe_writeback(&self) -> bool {
        self.writeback_block_reason.is_none() && !self.chunk_entries.is_empty()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PdfWritebackModel {
    pub source_text: String,
    pub writeback_slots: Vec<WritebackSlot>,
    pub template_signature: String,
    pub slot_structure_signature: String,
}

#[derive(Debug, Clone)]
struct PdfChunkEntry {
    page_number: u32,
    raw_text: String,
    text: String,
    separator_after: String,
    region_anchor: String,
}

#[derive(Debug, Clone)]
struct PdfLockedEntry {
    text: String,
    separator_after: String,
    region_anchor: String,
    protect_kind: &'static str,
}

#[derive(Debug, Clone)]
enum PdfTemplateEntry {
    Chunk(PdfChunkEntry),
    Locked(PdfLockedEntry),
}

#[derive(Debug, Clone)]
struct StructuredPdfSource {
    source_text: String,
    template: TextTemplate,
    chunk_entries: Vec<PdfChunkEntry>,
    writeback_block_reason: Option<String>,
}

impl PdfAdapter {
    pub(crate) fn load_writeback_source(
        pdf_bytes: &[u8],
    ) -> Result<LoadedPdfWritebackSource, String> {
        if pdf_bytes.is_empty() {
            return Err(PDF_EMPTY_ERROR.to_string());
        }

        match try_extract_structured_source(pdf_bytes) {
            Ok(structured) => Ok(LoadedPdfWritebackSource {
                source_text: structured.source_text,
                template: structured.template,
                writeback_block_reason: structured.writeback_block_reason,
                chunk_entries: structured.chunk_entries,
            }),
            Err(detail) => {
                let fallback_source_text = extract_fallback_text(pdf_bytes)?;
                Ok(LoadedPdfWritebackSource {
                    source_text: fallback_source_text.clone(),
                    template: build_fallback_template(&fallback_source_text),
                    writeback_block_reason: Some(pdf_writeback_block_reason(&detail)),
                    chunk_entries: Vec::new(),
                })
            }
        }
    }

    pub(crate) fn extract_writeback_model_from_source(
        loaded: &LoadedPdfWritebackSource,
    ) -> PdfWritebackModel {
        build_pdf_writeback_model(&loaded.template, &loaded.source_text)
    }

    pub(crate) fn write_updated_slots_with_source(
        pdf_bytes: &[u8],
        loaded: &LoadedPdfWritebackSource,
        updated_slots: &[WritebackSlot],
    ) -> Result<Vec<u8>, String> {
        if !loaded.supports_safe_writeback() {
            return Err(loaded
                .writeback_block_reason
                .clone()
                .unwrap_or_else(|| pdf_writeback_block_reason("当前 PDF 不支持安全写回。")));
        }

        let expected_text =
            textual_template::rebuild::rebuild_text(&loaded.template, updated_slots)?;
        let updated_chunk_texts = rebuild_region_texts(&loaded.chunk_entries, updated_slots)?;
        if updated_chunk_texts.len() != loaded.chunk_entries.len() {
            return Err("当前 PDF 槽位结构与原始文本块数量不一致，无法安全写回。".to_string());
        }

        let mut document =
            Document::load_mem(pdf_bytes).map_err(|error| format!("PDF 解析失败：{error}"))?;

        for (entry, updated_chunk_text) in
            loaded.chunk_entries.iter().zip(updated_chunk_texts.iter())
        {
            let search_text = replacement_search_text(&entry.raw_text);
            let updated_body =
                strip_known_separator_suffix(updated_chunk_text, &entry.separator_after);
            let updated_raw = normalize_text_against_chunk_layout(search_text, updated_body);
            document
                .replace_text(entry.page_number, search_text, &updated_raw, Some("?"))
                .map_err(|error| format!("PDF 写回失败：{error}"))?;
        }

        let mut output = Vec::new();
        document
            .save_to(&mut output)
            .map_err(|error| format!("PDF 写回失败：{error}"))?;

        let reloaded = Self::load_writeback_source(&output)?;
        if reloaded.source_text != expected_text {
            return Err(PDF_RELOAD_MISMATCH_ERROR.to_string());
        }

        Ok(output)
    }
}

fn build_pdf_writeback_model(template: &TextTemplate, source_text: &str) -> PdfWritebackModel {
    let built = textual_template::slots::build_slots(template);
    PdfWritebackModel {
        source_text: source_text.to_string(),
        writeback_slots: built.slots,
        template_signature: template.template_signature.clone(),
        slot_structure_signature: built.slot_structure_signature,
    }
}

fn try_extract_structured_source(pdf_bytes: &[u8]) -> Result<StructuredPdfSource, String> {
    let document =
        Document::load_mem(pdf_bytes).map_err(|error| format!("PDF 解析失败：{error}"))?;
    let pages = document.get_pages();
    if pages.is_empty() {
        return Err("未解析到任何页面内容。".to_string());
    }

    let page_count = pages.len();
    let mut chunk_entries = Vec::new();
    let mut template_entries = Vec::new();
    let mut writeback_issue = None::<String>;

    for (page_index, (page_number, page_id)) in pages.iter().map(|(n, id)| (*n, *id)).enumerate() {
        let extracted_chunks = document.extract_text_chunks(&[page_number]);
        let mut pending_separator = String::new();
        let mut seen_raw_chunks = HashSet::new();
        let mut page_chunk_entries = Vec::new();
        let mut page_locked_entries = Vec::new();

        for extracted in extracted_chunks {
            let raw_text = match extracted {
                Ok(raw_text) => raw_text,
                Err(_) => {
                    let lock_offset = page_chunk_entries.len() + page_locked_entries.len();
                    page_locked_entries
                        .push(pdf_decode_error_locked_entry(page_index, lock_offset));
                    continue;
                }
            };
            let normalized = normalize_extracted_chunk_text(&raw_text);
            let (text, separator_after) = split_text_and_trailing_separator(&normalized);

            if text.is_empty() {
                if !separator_after.is_empty() {
                    pending_separator.push_str(&separator_after);
                }
                continue;
            }

            if !seen_raw_chunks.insert(raw_text.clone()) && writeback_issue.is_none() {
                writeback_issue = Some(PDF_DUPLICATE_CHUNK_REASON.to_string());
            }

            let page_visible_index = page_chunk_entries.len();
            let region_anchor = format!("pdf:p{page_index}:b{page_visible_index}:r0");
            let mut combined_separator = String::new();
            if !pending_separator.is_empty() {
                combined_separator.push_str(&pending_separator);
                pending_separator.clear();
            }
            combined_separator.push_str(&separator_after);

            page_chunk_entries.push(PdfChunkEntry {
                page_number,
                raw_text,
                text,
                separator_after: combined_separator,
                region_anchor,
            });
        }

        if let Some(last) = page_chunk_entries.last_mut() {
            last.separator_after.push_str(&pending_separator);
        } else if let Some(last) = page_locked_entries.last_mut() {
            last.separator_after.push_str(&pending_separator);
        }

        let mut page_feature_locked_entries = build_page_locked_entries(
            &document,
            page_id,
            page_index,
            page_chunk_entries.len() + page_locked_entries.len(),
        );
        page_locked_entries.append(&mut page_feature_locked_entries);

        if page_index + 1 < page_count {
            if let Some(last) = page_locked_entries.last_mut() {
                last.separator_after.push_str("\n\n");
            } else if let Some(last) = page_chunk_entries.last_mut() {
                last.separator_after.push_str("\n\n");
            }
        }

        for entry in page_chunk_entries {
            chunk_entries.push(entry.clone());
            template_entries.push(PdfTemplateEntry::Chunk(entry));
        }
        for entry in page_locked_entries {
            template_entries.push(PdfTemplateEntry::Locked(entry));
        }
    }

    if template_entries.is_empty() {
        return Err(PDF_NO_VISIBLE_TEXT_ERROR.to_string());
    }
    if chunk_entries.is_empty() && writeback_issue.is_none() {
        writeback_issue = Some(PDF_NO_EDITABLE_CHUNK_REASON.to_string());
    }

    let template = build_template_from_entries(&template_entries);
    let source_text = source_text_from_template_entries(&template_entries);

    Ok(StructuredPdfSource {
        source_text,
        template,
        chunk_entries,
        writeback_block_reason: writeback_issue.map(|detail| pdf_writeback_block_reason(&detail)),
    })
}

fn build_template_from_entries(entries: &[PdfTemplateEntry]) -> TextTemplate {
    let blocks = entries
        .iter()
        .map(|entry| match entry {
            PdfTemplateEntry::Chunk(chunk) => TextTemplateBlock {
                anchor: region_block_anchor(&chunk.region_anchor).to_string(),
                kind: "chunk".to_string(),
                regions: vec![TextTemplateRegion {
                    anchor: chunk.region_anchor.clone(),
                    text: chunk.text.clone(),
                    editable: true,
                    role: WritebackSlotRole::EditableText,
                    presentation: None,
                    split_mode: TextRegionSplitMode::BoundaryAware,
                    separator_after: chunk.separator_after.clone(),
                }],
            },
            PdfTemplateEntry::Locked(locked) => TextTemplateBlock {
                anchor: region_block_anchor(&locked.region_anchor).to_string(),
                kind: "locked".to_string(),
                regions: vec![TextTemplateRegion {
                    anchor: locked.region_anchor.clone(),
                    text: locked.text.clone(),
                    editable: false,
                    role: WritebackSlotRole::InlineObject,
                    presentation: pdf_placeholder_presentation(locked.protect_kind),
                    split_mode: TextRegionSplitMode::Atomic,
                    separator_after: locked.separator_after.clone(),
                }],
            },
        })
        .collect::<Vec<_>>();
    TextTemplate::new("pdf", blocks)
}

fn source_text_from_template_entries(entries: &[PdfTemplateEntry]) -> String {
    entries
        .iter()
        .map(|entry| match entry {
            PdfTemplateEntry::Chunk(chunk) => format!("{}{}", chunk.text, chunk.separator_after),
            PdfTemplateEntry::Locked(locked) => {
                format!("{}{}", locked.text, locked.separator_after)
            }
        })
        .collect::<String>()
}

fn build_fallback_template(source_text: &str) -> TextTemplate {
    if source_text.is_empty() {
        return TextTemplate::new("pdf", Vec::new());
    }

    let blocks = split_text_chunks_by_paragraph_separator(source_text)
        .into_iter()
        .enumerate()
        .map(|(index, chunk)| {
            let (text, separator_after) = split_text_and_trailing_separator(chunk);
            let block_anchor = format!("pdf:fallback:p{index}");
            TextTemplateBlock {
                anchor: block_anchor.clone(),
                kind: "fallback".to_string(),
                regions: vec![TextTemplateRegion {
                    anchor: format!("{block_anchor}:r0"),
                    text,
                    editable: true,
                    role: WritebackSlotRole::EditableText,
                    presentation: None,
                    split_mode: TextRegionSplitMode::BoundaryAware,
                    separator_after,
                }],
            }
        })
        .collect::<Vec<_>>();

    TextTemplate::new("pdf", blocks)
}

fn rebuild_region_texts(
    chunk_entries: &[PdfChunkEntry],
    updated_slots: &[WritebackSlot],
) -> Result<Vec<String>, String> {
    let mut region_map = HashMap::new();
    for slot in updated_slots {
        let region_anchor = slot_region_anchor(slot)?;
        let entry = region_map
            .entry(region_anchor.to_string())
            .or_insert_with(String::new);
        entry.push_str(&slot.text);
        entry.push_str(&slot.separator_after);
    }

    chunk_entries
        .iter()
        .map(|entry| {
            region_map
                .get(&entry.region_anchor)
                .cloned()
                .ok_or_else(|| format!("PDF 模板区域缺少对应槽位：{}。", entry.region_anchor))
        })
        .collect()
}

fn slot_region_anchor(slot: &WritebackSlot) -> Result<&str, String> {
    let anchor = slot
        .anchor
        .as_deref()
        .ok_or_else(|| "槽位缺少 anchor，无法写回 PDF。".to_string())?;
    anchor
        .rsplit_once(":s")
        .map(|(region_anchor, _)| region_anchor)
        .ok_or_else(|| format!("槽位 anchor 不是 region-slot 形式：{anchor}。"))
}

fn region_block_anchor(region_anchor: &str) -> &str {
    region_anchor
        .rsplit_once(":r")
        .map(|(block_anchor, _)| block_anchor)
        .unwrap_or(region_anchor)
}

#[derive(Debug, Clone, Copy)]
enum PdfPlaceholderKind {
    Link,
    Image,
    Object,
    Graphics,
}

impl PdfPlaceholderKind {
    fn text(self) -> &'static str {
        match self {
            Self::Link => PDF_LINK_PLACEHOLDER,
            Self::Image => PDF_IMAGE_PLACEHOLDER,
            Self::Object => PDF_OBJECT_PLACEHOLDER,
            Self::Graphics => PDF_GRAPHICS_PLACEHOLDER,
        }
    }

    fn protect_kind(self) -> &'static str {
        match self {
            Self::Link => PDF_LINK_PROTECT_KIND,
            Self::Image => PDF_IMAGE_PROTECT_KIND,
            Self::Object => PDF_OBJECT_PROTECT_KIND,
            Self::Graphics => PDF_GRAPHICS_PROTECT_KIND,
        }
    }
}

fn pdf_placeholder_presentation(kind: &str) -> Option<TextPresentation> {
    Some(TextPresentation {
        bold: false,
        italic: false,
        underline: false,
        href: None,
        protect_kind: Some(kind.to_string()),
        writeback_key: None,
    })
}

fn build_page_locked_entries(
    document: &Document,
    page_id: ObjectId,
    page_index: usize,
    base_offset: usize,
) -> Vec<PdfLockedEntry> {
    let kinds = detect_page_placeholder_kinds(document, page_id);
    kinds
        .into_iter()
        .enumerate()
        .map(|(index, kind)| PdfLockedEntry {
            text: kind.text().to_string(),
            separator_after: "\n".to_string(),
            region_anchor: format!("pdf:p{page_index}:o{}:r0", base_offset + index),
            protect_kind: kind.protect_kind(),
        })
        .collect()
}

fn pdf_decode_error_locked_entry(page_index: usize, lock_offset: usize) -> PdfLockedEntry {
    PdfLockedEntry {
        text: PDF_OBJECT_PLACEHOLDER.to_string(),
        separator_after: "\n".to_string(),
        region_anchor: format!("pdf:p{page_index}:o{lock_offset}:r0"),
        protect_kind: PDF_OBJECT_PROTECT_KIND,
    }
}

fn detect_page_placeholder_kinds(
    document: &Document,
    page_id: ObjectId,
) -> Vec<PdfPlaceholderKind> {
    let mut kinds = Vec::new();
    if page_has_link_annotations(document, page_id) {
        kinds.push(PdfPlaceholderKind::Link);
    }
    if page_has_image_xobjects(document, page_id) {
        kinds.push(PdfPlaceholderKind::Image);
    }
    if page_has_non_image_xobjects(document, page_id) {
        kinds.push(PdfPlaceholderKind::Object);
    }
    if page_has_graphics_paths(document, page_id) {
        kinds.push(PdfPlaceholderKind::Graphics);
    }
    kinds
}

fn page_has_link_annotations(document: &Document, page_id: ObjectId) -> bool {
    document
        .get_page_annotations(page_id)
        .map(|annotations| {
            annotations.iter().any(|annotation| {
                annotation
                    .get(b"Subtype")
                    .and_then(Object::as_name)
                    .is_ok_and(|name| name == b"Link")
            })
        })
        .unwrap_or(false)
}

fn page_has_image_xobjects(document: &Document, page_id: ObjectId) -> bool {
    document
        .get_page_images(page_id)
        .map(|images| !images.is_empty())
        .unwrap_or(false)
}

fn page_has_non_image_xobjects(document: &Document, page_id: ObjectId) -> bool {
    let (resource_dict, resource_ids) = match document.get_page_resources(page_id) {
        Ok(resources) => resources,
        Err(_) => return false,
    };

    let mut resources = Vec::new();
    if let Some(resource) = resource_dict {
        resources.push(resource);
    }
    for resource_id in resource_ids {
        if let Ok(resource) = document.get_dictionary(resource_id) {
            resources.push(resource);
        }
    }

    resources
        .iter()
        .any(|resource| resource_has_non_image_xobject(document, resource))
}

fn resource_has_non_image_xobject(document: &Document, resources: &Dictionary) -> bool {
    let xobjects = match resources.get(b"XObject") {
        Ok(Object::Reference(id)) => match document.get_object(*id).and_then(Object::as_dict) {
            Ok(dict) => dict,
            Err(_) => return true,
        },
        Ok(Object::Dictionary(dict)) => dict,
        Ok(_) => return false,
        Err(_) => return false,
    };

    for (_, xobject) in xobjects.iter() {
        let stream = match xobject {
            Object::Reference(id) => match document.get_object(*id).and_then(Object::as_stream) {
                Ok(stream) => stream,
                Err(_) => return true,
            },
            Object::Stream(stream) => stream,
            _ => return true,
        };
        let subtype = stream.dict.get(b"Subtype").and_then(Object::as_name).ok();
        if !matches!(subtype, Some(name) if name == b"Image") {
            return true;
        }
    }

    false
}

fn page_has_graphics_paths(document: &Document, page_id: ObjectId) -> bool {
    let content = match document.get_and_decode_page_content(page_id) {
        Ok(content) => content,
        Err(_) => return false,
    };

    content.operations.iter().any(|operation| {
        matches!(
            operation.operator.as_str(),
            "m" | "l"
                | "c"
                | "v"
                | "y"
                | "h"
                | "re"
                | "S"
                | "s"
                | "f"
                | "F"
                | "f*"
                | "B"
                | "B*"
                | "b"
                | "b*"
                | "n"
                | "W"
                | "W*"
                | "sh"
        )
    })
}

fn extract_fallback_text(pdf_bytes: &[u8]) -> Result<String, String> {
    let pages = pdf_extract::extract_text_from_mem_by_pages(pdf_bytes).map_err(|error| {
        format!(
            "PDF 文本抽取失败：{}",
            normalize_pdf_extract_error(&error.to_string())
        )
    })?;

    if pages.is_empty() {
        return Err("PDF 文本抽取失败：未解析到任何页面内容。".to_string());
    }

    let mut out = String::new();
    for (index, page) in pages.into_iter().enumerate() {
        if index > 0 {
            out.push_str("\n\n");
        }
        out.push_str(&normalize_extracted_chunk_text(&page));
    }

    let cleaned = out.trim_matches('\u{feff}').to_string();
    if cleaned.trim().is_empty() {
        return Err(PDF_NO_VISIBLE_TEXT_ERROR.to_string());
    }

    Ok(cleaned)
}

fn normalize_extracted_chunk_text(text: &str) -> String {
    let normalized = text
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim_matches('\u{feff}')
        .to_string();
    rewrite::strip_trailing_spaces_per_line(&normalized)
}

fn normalize_text_against_chunk_layout(source_text: &str, updated_text: &str) -> String {
    let line_ending = rewrite::detect_line_ending(source_text);
    let mut normalized = updated_text.to_string();
    if !rewrite::has_trailing_spaces_per_line(source_text) {
        normalized = rewrite::strip_trailing_spaces_per_line(&normalized);
    }
    rewrite::convert_line_endings(&normalized, line_ending)
}

fn replacement_search_text(raw_text: &str) -> &str {
    raw_text.trim_end_matches(['\r', '\n'])
}

fn strip_known_separator_suffix<'a>(text: &'a str, separator_after: &str) -> &'a str {
    if separator_after.is_empty() {
        return text;
    }
    text.strip_suffix(separator_after).unwrap_or(text)
}

fn pdf_writeback_block_reason(detail: &str) -> String {
    format!("{PDF_SAFE_WRITEBACK_UNSUPPORTED_PREFIX}原因：{detail}")
}

fn normalize_pdf_extract_error(error: &str) -> String {
    let lower = error.to_ascii_lowercase();
    if lower.contains("tounicode") {
        return PDF_TOUNICODE_MISSING_REASON.to_string();
    }

    "文本层编码不完整或字体映射异常。".to_string()
}

#[cfg(test)]
mod tests {
    use crate::test_support::{build_minimal_pdf, build_minimal_pdf_with_features};

    #[test]
    fn load_writeback_source_allows_safe_pdf_chunks() {
        let bytes = build_minimal_pdf(&["Alpha line", "Beta line"]);

        let loaded = super::PdfAdapter::load_writeback_source(&bytes).expect("load pdf");
        let model = super::PdfAdapter::extract_writeback_model_from_source(&loaded);

        assert_eq!(loaded.template.kind, "pdf");
        assert!(loaded.supports_safe_writeback());
        assert!(loaded.writeback_block_reason.is_none());
        assert_eq!(loaded.source_text, "Alpha line\nBeta line\n");
        assert_eq!(model.writeback_slots.len(), 2);
        assert_eq!(
            model
                .writeback_slots
                .iter()
                .map(|slot| slot.anchor.clone().unwrap_or_default())
                .collect::<Vec<_>>(),
            vec!["pdf:p0:b0:r0:s0", "pdf:p0:b1:r0:s0"]
        );
    }

    #[test]
    fn load_writeback_source_blocks_duplicate_chunks_on_same_page() {
        let bytes = build_minimal_pdf(&["Repeat", "Repeat"]);

        let loaded = super::PdfAdapter::load_writeback_source(&bytes).expect("load pdf");

        assert!(!loaded.supports_safe_writeback());
        assert!(loaded
            .writeback_block_reason
            .as_deref()
            .is_some_and(|message| message.contains("重复文本块")));
        assert_eq!(loaded.template.kind, "pdf");
        assert_eq!(loaded.source_text, "Repeat\nRepeat\n");
    }

    #[test]
    fn write_updated_slots_with_source_roundtrips_safe_pdf() {
        let bytes = build_minimal_pdf(&["Alpha line", "Beta line"]);
        let loaded = super::PdfAdapter::load_writeback_source(&bytes).expect("load pdf");
        let mut slots =
            super::PdfAdapter::extract_writeback_model_from_source(&loaded).writeback_slots;
        slots[0].text = "Alpha revised".to_string();

        let updated = super::PdfAdapter::write_updated_slots_with_source(&bytes, &loaded, &slots)
            .expect("write updated pdf");
        let reloaded = super::PdfAdapter::load_writeback_source(&updated).expect("reload pdf");

        assert_eq!(reloaded.source_text, "Alpha revised\nBeta line\n");
    }

    #[test]
    fn load_writeback_source_keeps_locked_placeholders_with_editable_chunks() {
        let bytes = build_minimal_pdf_with_features(&["Alpha line"], true, true);

        let loaded = super::PdfAdapter::load_writeback_source(&bytes).expect("load pdf");
        let model = super::PdfAdapter::extract_writeback_model_from_source(&loaded);
        let link_slot = model
            .writeback_slots
            .iter()
            .find(|slot| slot.text == "[链接]")
            .expect("link placeholder slot");
        let graphics_slot = model
            .writeback_slots
            .iter()
            .find(|slot| slot.text == "[图形]")
            .expect("graphics placeholder slot");

        assert!(loaded.supports_safe_writeback());
        assert!(loaded.source_text.contains("Alpha line"));
        assert!(loaded.source_text.contains("[链接]"));
        assert!(loaded.source_text.contains("[图形]"));
        assert!(!link_slot.editable);
        assert_eq!(
            link_slot.role,
            crate::rewrite_unit::WritebackSlotRole::InlineObject
        );
        assert_eq!(
            link_slot
                .presentation
                .as_ref()
                .and_then(|value| value.protect_kind.as_deref()),
            Some("pdf-link")
        );
        assert!(!graphics_slot.editable);
        assert_eq!(
            graphics_slot
                .presentation
                .as_ref()
                .and_then(|value| value.protect_kind.as_deref()),
            Some("pdf-graphics")
        );
    }

    #[test]
    fn load_writeback_source_blocks_pdf_with_only_locked_placeholders() {
        let bytes = build_minimal_pdf_with_features(&[], true, true);

        let loaded = super::PdfAdapter::load_writeback_source(&bytes).expect("load pdf");
        let model = super::PdfAdapter::extract_writeback_model_from_source(&loaded);

        assert!(!loaded.supports_safe_writeback());
        assert!(loaded
            .writeback_block_reason
            .as_deref()
            .is_some_and(|message| message.contains("未抽取到可写回的文本层块")));
        assert!(loaded.source_text.contains("[链接]"));
        assert!(loaded.source_text.contains("[图形]"));
        assert!(model.writeback_slots.iter().all(|slot| !slot.editable));
    }

    #[test]
    fn normalize_pdf_extract_error_maps_missing_tounicode() {
        let message =
            super::normalize_pdf_extract_error("missing required dictionary key \"ToUnicode\"");
        assert!(message.contains("ToUnicode"));
        assert!(!message.contains("missing required dictionary key"));
    }

    #[test]
    fn pdf_decode_error_locked_entry_uses_object_placeholder_anchor() {
        let entry = super::pdf_decode_error_locked_entry(2, 5);
        assert_eq!(entry.text, "[对象]");
        assert_eq!(entry.region_anchor, "pdf:p2:o5:r0");
        assert_eq!(entry.protect_kind, "pdf-object");
    }
}
