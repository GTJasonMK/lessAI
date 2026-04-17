use crate::rewrite_unit::{WritebackSlot, WritebackSlotRole};

use super::{
    display::{build_display_blocks, DisplayBlockKind},
    model::{WritebackBlockTemplate, WritebackParagraphTemplate, WritebackRegionTemplate},
};

const DOCX_BLOCK_SEPARATOR: &str = "\n\n";

pub(super) fn build_writeback_slots(
    blocks: &[WritebackBlockTemplate],
    rewrite_headings: bool,
) -> Vec<WritebackSlot> {
    let display_blocks = build_display_blocks(blocks);
    let mut slots = Vec::new();

    for (display_index, display_block) in display_blocks.iter().enumerate() {
        let append_block_separator = display_index + 1 < display_blocks.len();
        match display_block.kind {
            DisplayBlockKind::Paragraph { block_index } => {
                let WritebackBlockTemplate::Paragraph(paragraph) = &blocks[block_index] else {
                    continue;
                };
                if display_block.region_indices.is_empty() {
                    slots.push(paragraph_break_slot(slots.len(), block_index, append_block_separator));
                    continue;
                }
                slots.extend(build_paragraph_slots(
                    slots.len(),
                    block_index,
                    paragraph,
                    &display_block.region_indices,
                    append_block_separator,
                    rewrite_headings,
                ));
            }
            DisplayBlockKind::LockedBlock { block_index } => {
                let WritebackBlockTemplate::Locked(region) = &blocks[block_index] else {
                    continue;
                };
                slots.push(locked_block_slot(
                    slots.len(),
                    block_index,
                    region,
                    append_block_separator,
                ));
            }
        }
    }

    slots
}

fn paragraph_break_slot(order: usize, block_index: usize, append_block_separator: bool) -> WritebackSlot {
    WritebackSlot {
        id: format!("docx:p{block_index}:break"),
        order,
        text: String::new(),
        editable: false,
        role: WritebackSlotRole::ParagraphBreak,
        presentation: None,
        anchor: None,
        separator_after: paragraph_separator(append_block_separator),
    }
}

fn build_paragraph_slots(
    start_order: usize,
    block_index: usize,
    paragraph: &WritebackParagraphTemplate,
    region_indices: &[usize],
    append_block_separator: bool,
    rewrite_headings: bool,
) -> Vec<WritebackSlot> {
    let mut slots = Vec::with_capacity(region_indices.len());
    for (position, region_index) in region_indices.iter().copied().enumerate() {
        let Some(region) = paragraph.regions.get(region_index) else {
            continue;
        };
        let is_last = position + 1 == region_indices.len();
        let editable = !paragraph_is_locked(paragraph, region, rewrite_headings);
        slots.push(WritebackSlot {
            id: format!("docx:p{block_index}:r{region_index}"),
            order: start_order + slots.len(),
            text: region.text().to_string(),
            editable,
            role: region_role(region, editable),
            presentation: region.presentation().cloned(),
            anchor: None,
            separator_after: if is_last {
                paragraph_separator(append_block_separator)
            } else {
                String::new()
            },
        });
    }
    slots
}

fn locked_block_slot(
    order: usize,
    block_index: usize,
    region: &super::model::LockedRegionTemplate,
    append_block_separator: bool,
) -> WritebackSlot {
    WritebackSlot {
        id: format!("docx:block:{block_index}"),
        order,
        text: region.text.clone(),
        editable: false,
        role: locked_role(region.presentation.as_ref()),
        presentation: region.presentation.clone(),
        anchor: None,
        separator_after: paragraph_separator(append_block_separator),
    }
}

fn paragraph_is_locked(
    paragraph: &WritebackParagraphTemplate,
    region: &WritebackRegionTemplate,
    rewrite_headings: bool,
) -> bool {
    if paragraph.is_heading && !rewrite_headings {
        return true;
    }
    region.skip_rewrite()
}

fn region_role(region: &WritebackRegionTemplate, editable: bool) -> WritebackSlotRole {
    if editable {
        return WritebackSlotRole::EditableText;
    }
    locked_role(region.presentation())
}

fn locked_role(presentation: Option<&crate::models::TextPresentation>) -> WritebackSlotRole {
    if presentation
        .and_then(|item| item.protect_kind.as_deref())
        .is_some()
    {
        return WritebackSlotRole::InlineObject;
    }
    WritebackSlotRole::LockedText
}

fn paragraph_separator(append_block_separator: bool) -> String {
    if append_block_separator {
        DOCX_BLOCK_SEPARATOR.to_string()
    } else {
        String::new()
    }
}
