use crate::{
    rewrite_unit::{WritebackSlot, WritebackSlotRole},
    text_boundaries::split_text_chunks_for_rewrite_slots,
};

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
                    slots.push(paragraph_break_slot(
                        slots.len(),
                        block_index,
                        append_block_separator,
                    ));
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

fn paragraph_break_slot(
    order: usize,
    block_index: usize,
    append_block_separator: bool,
) -> WritebackSlot {
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
    let mut slots = Vec::new();
    for (position, region_index) in region_indices.iter().copied().enumerate() {
        let Some(region) = paragraph.regions.get(region_index) else {
            continue;
        };
        let is_last = position + 1 == region_indices.len();
        let editable = !paragraph_is_locked(paragraph, region, rewrite_headings);
        let anchor = format!("docx:p{block_index}:r{region_index}");
        let mut fragments = split_region_slot_fragments(region.text(), editable);
        if let Some(last) = fragments.last_mut() {
            if is_last {
                last.separator_after
                    .push_str(&paragraph_separator(append_block_separator));
            }
        }

        let split_count = fragments.len();
        for (fragment_index, fragment) in fragments.into_iter().enumerate() {
            let slot_editable = editable && text_has_visible_content(&fragment.text);
            slots.push(WritebackSlot {
                id: slot_id(&anchor, fragment_index, split_count),
                order: start_order + slots.len(),
                text: fragment.text,
                editable: slot_editable,
                role: region_role(region, slot_editable),
                presentation: region.presentation().cloned(),
                anchor: Some(anchor.clone()),
                separator_after: fragment.separator_after,
            });
        }
    }
    slots
}

fn slot_id(anchor: &str, fragment_index: usize, split_count: usize) -> String {
    if split_count == 1 {
        return anchor.to_string();
    }
    format!("{anchor}:s{fragment_index}")
}

fn text_has_visible_content(text: &str) -> bool {
    !text.trim().is_empty()
}

#[derive(Debug)]
struct SlotFragment {
    text: String,
    separator_after: String,
}

fn split_region_slot_fragments(text: &str, editable: bool) -> Vec<SlotFragment> {
    let fragments = split_region_line_fragments(text);
    if !editable {
        return fragments;
    }
    split_editable_fragments_by_clause_boundary(fragments)
}

fn split_region_line_fragments(text: &str) -> Vec<SlotFragment> {
    if !text.contains('\n') {
        return vec![SlotFragment {
            text: text.to_string(),
            separator_after: String::new(),
        }];
    }

    let mut fragments = Vec::new();
    let mut current = String::new();
    let mut leading_separator = String::new();

    for ch in text.chars() {
        if ch == '\n' {
            if !current.is_empty() {
                fragments.push(SlotFragment {
                    text: std::mem::take(&mut current),
                    separator_after: "\n".to_string(),
                });
                continue;
            }
            if let Some(last) = fragments.last_mut() {
                last.separator_after.push('\n');
            } else {
                leading_separator.push('\n');
            }
            continue;
        }

        if !leading_separator.is_empty() {
            fragments.push(SlotFragment {
                text: String::new(),
                separator_after: std::mem::take(&mut leading_separator),
            });
        }
        current.push(ch);
    }

    if !current.is_empty() || fragments.is_empty() {
        fragments.push(SlotFragment {
            text: current,
            separator_after: leading_separator,
        });
    } else if !leading_separator.is_empty() {
        if let Some(last) = fragments.last_mut() {
            last.separator_after.push_str(&leading_separator);
        }
    }

    fragments
}

fn split_editable_fragments_by_clause_boundary(fragments: Vec<SlotFragment>) -> Vec<SlotFragment> {
    let mut atomic = Vec::new();

    for fragment in fragments {
        if !text_has_visible_content(&fragment.text) {
            atomic.push(fragment);
            continue;
        }

        let parts = split_text_chunks_for_rewrite_slots(&fragment.text);
        if parts.len() <= 1 {
            atomic.push(fragment);
            continue;
        }

        append_atomic_parts(&mut atomic, parts, fragment.separator_after);
    }

    atomic
}

fn append_atomic_parts(target: &mut Vec<SlotFragment>, parts: Vec<&str>, final_separator: String) {
    let last_index = parts.len().saturating_sub(1);
    for (index, part) in parts.into_iter().enumerate() {
        target.push(SlotFragment {
            text: part.to_string(),
            separator_after: if index == last_index {
                final_separator.clone()
            } else {
                String::new()
            },
        });
    }
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

pub(super) fn paragraph_is_locked(
    paragraph: &WritebackParagraphTemplate,
    region: &WritebackRegionTemplate,
    rewrite_headings: bool,
) -> bool {
    if paragraph.is_heading && !rewrite_headings {
        return true;
    }
    region.skip_rewrite()
}

pub(super) fn region_role(
    region: &WritebackRegionTemplate,
    editable: bool,
) -> WritebackSlotRole {
    if editable {
        return WritebackSlotRole::EditableText;
    }
    locked_role(region.presentation())
}

pub(super) fn locked_role(
    presentation: Option<&crate::models::TextPresentation>,
) -> WritebackSlotRole {
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
