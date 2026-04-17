use std::collections::HashMap;

use super::{SlotUpdate, WritebackSlot};

pub(crate) fn merged_text_from_slots(slots: &[WritebackSlot]) -> String {
    slots
        .iter()
        .map(|slot| format!("{}{}", slot.text, slot.separator_after))
        .collect()
}

pub(crate) fn apply_slot_updates(
    slots: &[WritebackSlot],
    updates: &[SlotUpdate],
) -> Result<Vec<WritebackSlot>, String> {
    let mut next = slots.to_vec();
    let positions = slot_positions(slots);

    for update in updates {
        let Some(position) = positions.get(update.slot_id.as_str()).copied() else {
            return Err(format!("未知 slot_id：{}。", update.slot_id));
        };
        let slot = &mut next[position];
        if !slot.editable {
            return Err(format!("locked slot 不允许修改：{}。", slot.id));
        }
        slot.text = update.text.clone();
    }

    Ok(next)
}

fn slot_positions(slots: &[WritebackSlot]) -> HashMap<String, usize> {
    slots.iter()
        .enumerate()
        .map(|(index, slot)| (slot.id.clone(), index))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{apply_slot_updates, merged_text_from_slots};
    use crate::rewrite_unit::{SlotUpdate, WritebackSlot};

    #[test]
    fn apply_slot_updates_updates_editable_slot_text_and_preserves_layout() {
        let mut slots = vec![
            WritebackSlot::editable("slot-1", 0, "原文"),
            WritebackSlot::locked("slot-2", 1, "[公式]"),
            WritebackSlot::editable("slot-3", 2, "后文"),
        ];
        slots[0].separator_after = "\n\n".to_string();

        let updated = apply_slot_updates(
            &slots,
            &[SlotUpdate::new("slot-1", "改写后"), SlotUpdate::new("slot-3", "尾段")],
        )
        .expect("slot updates should succeed");

        assert_eq!(updated[0].text, "改写后");
        assert_eq!(updated[0].separator_after, "\n\n");
        assert_eq!(updated[2].text, "尾段");
        assert_eq!(merged_text_from_slots(&updated), "改写后\n\n[公式]尾段");
    }

    #[test]
    fn apply_slot_updates_rejects_locked_slot_update() {
        let slots = vec![WritebackSlot::locked("slot-locked", 0, "[分页符]")];

        let error = apply_slot_updates(&slots, &[SlotUpdate::new("slot-locked", "改坏")])
            .expect_err("locked slot must be rejected");

        assert!(error.contains("locked slot"));
    }

    #[test]
    fn apply_slot_updates_rejects_unknown_slot_id() {
        let slots = vec![WritebackSlot::editable("slot-1", 0, "正文")];

        let error = apply_slot_updates(&slots, &[SlotUpdate::new("slot-x", "改写")])
            .expect_err("unknown slot id must be rejected");

        assert!(error.contains("未知 slot_id"));
    }
}
