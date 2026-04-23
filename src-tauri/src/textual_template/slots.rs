use crate::{
    rewrite_unit::{WritebackSlot, WritebackSlotRole},
    text_boundaries::{
        contains_paragraph_separator, split_text_and_trailing_separator,
        split_text_chunks_by_paragraph_separator, split_text_chunks_for_rewrite_slots,
    },
};

use super::{
    models::{TextRegionSplitMode, TextTemplate, TextTemplateRegion},
    signature::compute_slot_structure_signature,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltSlots {
    pub slots: Vec<WritebackSlot>,
    pub slot_structure_signature: String,
}

pub(crate) fn build_slots(template: &TextTemplate) -> BuiltSlots {
    let mut slots = Vec::new();
    for block in &template.blocks {
        for region in &block.regions {
            push_region_slots(&mut slots, region);
        }
    }

    BuiltSlots {
        slot_structure_signature: compute_slot_structure_signature(&slots),
        slots,
    }
}

fn push_region_slots(slots: &mut Vec<WritebackSlot>, region: &TextTemplateRegion) {
    for (split_index, chunk) in split_region_chunks(region).into_iter().enumerate() {
        let (text, separator_after) = split_chunk_body_and_separator(&chunk);
        if text.is_empty() && !separator_after.is_empty() {
            if let Some(last) = slots.last_mut() {
                last.separator_after.push_str(&separator_after);
                continue;
            }
        }
        slots.push(build_slot(
            slots.len(),
            split_index,
            region,
            text,
            separator_after,
        ));
    }
}

fn split_region_chunks(region: &TextTemplateRegion) -> Vec<String> {
    let combined = format!("{}{}", region.text, region.separator_after);
    if combined.is_empty() {
        return Vec::new();
    }

    let chunks = if !region.editable {
        split_text_chunks_by_paragraph_separator(&combined)
    } else {
        match region.split_mode {
            TextRegionSplitMode::BoundaryAware => split_text_chunks_for_rewrite_slots(&combined),
            TextRegionSplitMode::Atomic => vec![combined.as_str()],
        }
    };
    chunks.into_iter().map(|chunk| chunk.to_string()).collect()
}

fn build_slot(
    order: usize,
    split_index: usize,
    region: &TextTemplateRegion,
    text: String,
    separator_after: String,
) -> WritebackSlot {
    let text_empty = text.is_empty();
    let whitespace_only = !text_empty && text.chars().all(|ch| ch.is_whitespace());
    let editable = region.editable && !whitespace_only && !text_empty;
    let anchor = format!("{}:s{split_index}", region.anchor);

    WritebackSlot {
        id: anchor.clone(),
        order,
        text,
        editable,
        role: slot_role(region, editable, whitespace_only, &separator_after),
        presentation: region.presentation.clone(),
        anchor: Some(anchor),
        separator_after,
    }
}

fn slot_role(
    region: &TextTemplateRegion,
    editable: bool,
    whitespace_only: bool,
    separator_after: &str,
) -> WritebackSlotRole {
    if !editable && contains_paragraph_separator(separator_after) {
        return WritebackSlotRole::ParagraphBreak;
    }
    if editable {
        return region.role.clone();
    }
    if whitespace_only && region.role == WritebackSlotRole::EditableText {
        return WritebackSlotRole::LockedText;
    }
    region.role.clone()
}

fn split_chunk_body_and_separator(text: &str) -> (String, String) {
    split_text_and_trailing_separator(text)
}
