use super::*;

pub(super) fn load_docx_writeback_source(docx_bytes: &[u8]) -> Result<LoadedDocxWritebackSource, String> {
    let loaded = load_docx_document(docx_bytes)?;
    let blocks = extract_writeback_paragraph_templates(&loaded.document_xml, &loaded.support)?;
    Ok(LoadedDocxWritebackSource {
        document_xml: loaded.document_xml,
        blocks,
    })
}

pub(super) fn ensure_expected_docx_source_text(
    blocks: &[WritebackBlockTemplate],
    expected_source_text: &str,
) -> Result<(), String> {
    let current_source_text = build_writeback_source_text(blocks);
    if current_source_text == expected_source_text {
        return Ok(());
    }
    Err(
        "docx 原文件内容与当前会话不一致，文件可能已在外部发生变化。为避免误写，请重新导入。"
            .to_string(),
    )
}

pub(super) fn write_docx_with_regions(
    docx_bytes: &[u8],
    loaded: &LoadedDocxWritebackSource,
    expected_source_text: &str,
    updated_regions: &[TextRegion],
) -> Result<Vec<u8>, String> {
    ensure_expected_docx_source_text(&loaded.blocks, expected_source_text)?;
    let updated_xml =
        rewrite_document_xml_with_regions(&loaded.document_xml, &loaded.blocks, updated_regions)?;
    replace_document_xml(docx_bytes, &updated_xml)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct RunStyle {
    pub(super) bold: bool,
    pub(super) italic: bool,
    pub(super) underline: bool,
}

#[cfg(test)]
pub(super) fn extract_regions_from_document_xml(
    xml: &str,
    support: &DocxSupportData,
    rewrite_headings: bool,
) -> Result<Vec<TextRegion>, String> {
    let blocks = extract_writeback_paragraph_templates(xml, support)?;
    Ok(flatten_writeback_blocks(&blocks, rewrite_headings))
}

pub(super) fn text_regions_from_writeback_slots(updated_slots: &[WritebackSlot]) -> Vec<TextRegion> {
    let mut regions = Vec::new();
    let mut current_anchor: Option<&str> = None;
    let mut current_body = String::new();
    let mut current_presentation = None;
    let mut current_role = None;
    let mut current_has_editable = false;

    for slot in updated_slots {
        let anchor = slot.anchor.as_deref().unwrap_or(slot.id.as_str());
        if current_anchor.is_some_and(|current| current != anchor) {
            regions.push(slot_group_region(
                std::mem::take(&mut current_body),
                current_has_editable,
                current_role.take(),
                current_presentation.take(),
            ));
            current_has_editable = false;
        }

        if current_anchor != Some(anchor) {
            current_anchor = Some(anchor);
            current_presentation = slot.presentation.clone();
            current_role = Some(slot.role.clone());
        }

        current_body.push_str(&slot.text);
        current_body.push_str(&slot.separator_after);
        current_has_editable |= slot.editable;
    }

    if current_anchor.is_some() {
        regions.push(slot_group_region(
            current_body,
            current_has_editable,
            current_role,
            current_presentation,
        ));
    }

    regions
}

pub(super) fn slot_group_region(
    body: String,
    editable: bool,
    role: Option<crate::rewrite_unit::WritebackSlotRole>,
    presentation: Option<TextPresentation>,
) -> TextRegion {
    if editable {
        return TextRegion::editable(body).with_presentation(presentation);
    }

    match role.unwrap_or(crate::rewrite_unit::WritebackSlotRole::LockedText) {
        crate::rewrite_unit::WritebackSlotRole::InlineObject => {
            TextRegion::inline_object(body).with_presentation(presentation)
        }
        crate::rewrite_unit::WritebackSlotRole::SyntaxToken => {
            TextRegion::syntax_token(body).with_presentation(presentation)
        }
        _ => locked_region_from_presentation(body, presentation),
    }
}

pub(super) fn locked_region_from_presentation(
    body: String,
    presentation: Option<TextPresentation>,
) -> TextRegion {
    if presentation
        .as_ref()
        .and_then(|value| value.protect_kind.as_deref())
        .is_some()
    {
        return TextRegion::inline_object(body).with_presentation(presentation);
    }
    TextRegion::locked_text(body).with_presentation(presentation)
}
