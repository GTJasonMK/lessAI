use crate::{
    models::{DocumentFormat, DocumentSession},
};

use super::{
    projection::{apply_slot_updates, merged_text_from_slots},
    RewriteUnit, RewriteUnitRequest, RewriteUnitSlot, SlotUpdate, WritebackSlot,
};

const REWRITE_UNIT_NOT_FOUND_ERROR: &str = "未找到对应的改写单元。";
const WRITEBACK_SLOT_NOT_FOUND_ERROR: &str = "未找到对应的写回槽位。";

pub(crate) fn find_rewrite_unit<'a>(
    session: &'a DocumentSession,
    rewrite_unit_id: &str,
) -> Result<&'a RewriteUnit, String> {
    session
        .rewrite_units
        .iter()
        .find(|unit| unit.id == rewrite_unit_id)
        .ok_or_else(|| REWRITE_UNIT_NOT_FOUND_ERROR.to_string())
}

fn find_writeback_slot<'a>(
    session: &'a DocumentSession,
    slot_id: &str,
) -> Result<&'a WritebackSlot, String> {
    session
        .writeback_slots
        .iter()
        .find(|slot| slot.id == slot_id)
        .ok_or_else(|| WRITEBACK_SLOT_NOT_FOUND_ERROR.to_string())
}

fn rewrite_unit_slots(
    session: &DocumentSession,
    rewrite_unit_id: &str,
) -> Result<Vec<WritebackSlot>, String> {
    let unit = find_rewrite_unit(session, rewrite_unit_id)?;
    unit.slot_ids
        .iter()
        .map(|slot_id| find_writeback_slot(session, slot_id).cloned())
        .collect()
}

pub(crate) fn rewrite_unit_text(
    session: &DocumentSession,
    rewrite_unit_id: &str,
) -> Result<String, String> {
    rewrite_unit_slots(session, rewrite_unit_id).map(|slots| merged_text_from_slots(&slots))
}

fn unit_slot_projection(
    session: &DocumentSession,
    rewrite_unit_id: &str,
    updates: &[SlotUpdate],
) -> Result<Vec<WritebackSlot>, String> {
    let unit = find_rewrite_unit(session, rewrite_unit_id)?;
    for update in updates {
        if !unit.slot_ids.iter().any(|slot_id| slot_id == &update.slot_id) {
            return Err(format!(
                "改写结果越过了改写单元边界：{} 不属于 {}。",
                update.slot_id, rewrite_unit_id
            ));
        }
    }
    apply_slot_updates(&session.writeback_slots, updates)
}

pub(crate) fn rewrite_unit_text_with_updates(
    session: &DocumentSession,
    rewrite_unit_id: &str,
    updates: &[SlotUpdate],
) -> Result<String, String> {
    unit_slot_projection(session, rewrite_unit_id, updates).map(|slots| {
        let visible = slots
            .into_iter()
            .filter(|slot| {
                find_rewrite_unit(session, rewrite_unit_id)
                    .map(|unit| unit.slot_ids.iter().any(|slot_id| slot_id == &slot.id))
                    .unwrap_or(false)
            })
            .collect::<Vec<_>>();
        merged_text_from_slots(&visible)
    })
}

pub(crate) fn build_rewrite_unit_request(
    session: &DocumentSession,
    rewrite_unit_id: &str,
    format: DocumentFormat,
) -> Result<RewriteUnitRequest, String> {
    let slots = rewrite_unit_slots(session, rewrite_unit_id)?;
    Ok(build_rewrite_unit_request_from_slots(
        rewrite_unit_id,
        &slots,
        format,
    ))
}

pub(crate) fn build_rewrite_unit_request_from_slots(
    rewrite_unit_id: &str,
    slots: &[WritebackSlot],
    format: DocumentFormat,
) -> RewriteUnitRequest {
    let request_slots = slots
        .iter()
        .map(|slot| RewriteUnitSlot {
            slot_id: slot.id.clone(),
            text: slot.text.clone(),
            editable: slot.editable,
            role: slot.role.clone(),
        })
        .collect();
    RewriteUnitRequest::new(
        rewrite_unit_id,
        format_label(format),
        request_slots,
    )
}

fn format_label(format: DocumentFormat) -> &'static str {
    match format {
        DocumentFormat::PlainText => "plainText",
        DocumentFormat::Markdown => "markdown",
        DocumentFormat::Tex => "tex",
    }
}
