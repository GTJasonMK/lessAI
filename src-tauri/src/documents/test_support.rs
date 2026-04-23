use crate::{
    adapters, models,
    rewrite_unit::{WritebackSlot, WritebackSlotRole},
    text_boundaries::{
        contains_paragraph_separator, split_text_and_trailing_separator,
        split_text_chunks_by_paragraph_separator, split_text_chunks_for_rewrite_slots,
    },
};

pub(crate) fn writeback_slots_from_regions(regions: &[adapters::TextRegion]) -> Vec<WritebackSlot> {
    let mut slots = Vec::new();
    for region in regions {
        for body in split_region_slot_chunks(region) {
            push_region_slot_part(&mut slots, region, body);
        }
    }
    slots
}

fn push_region_slot_part(
    slots: &mut Vec<WritebackSlot>,
    region: &adapters::TextRegion,
    body: &str,
) {
    let (text, separator_after) = split_text_and_trailing_separator(body);
    if text.is_empty() && !separator_after.is_empty() {
        if let Some(last) = slots.last_mut() {
            last.separator_after.push_str(&separator_after);
            return;
        }
    }
    slots.push(build_writeback_slot(
        slots.len(),
        text,
        separator_after,
        region,
        region.presentation.clone(),
    ));
}

fn build_writeback_slot(
    index: usize,
    text: String,
    separator_after: String,
    region: &adapters::TextRegion,
    presentation: Option<models::TextPresentation>,
) -> WritebackSlot {
    let text_empty = text.is_empty();
    let whitespace_only = !text.is_empty() && text.chars().all(|ch| ch.is_whitespace());
    let editable = !region.skip_rewrite && !whitespace_only && !text_empty;

    WritebackSlot {
        id: format!("slot-{index}"),
        order: index,
        text,
        editable,
        role: slot_role(region, text_empty, editable, whitespace_only, &separator_after),
        presentation,
        anchor: None,
        separator_after,
    }
}

fn slot_role(
    region: &adapters::TextRegion,
    text_empty: bool,
    editable: bool,
    whitespace_only: bool,
    separator_after: &str,
) -> WritebackSlotRole {
    if text_empty && contains_paragraph_separator(separator_after) {
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

fn split_region_slot_chunks<'a>(region: &'a adapters::TextRegion) -> Vec<&'a str> {
    if region.skip_rewrite {
        return split_text_chunks_by_paragraph_separator(&region.body);
    }
    match region.split_mode {
        crate::textual_template::models::TextRegionSplitMode::BoundaryAware => {
            split_text_chunks_for_rewrite_slots(&region.body)
        }
        crate::textual_template::models::TextRegionSplitMode::Atomic => vec![region.body.as_str()],
    }
}
