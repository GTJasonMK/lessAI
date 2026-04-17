use crate::models::{SegmentationPreset, RewriteUnitStatus};

use super::{RewriteUnit, WritebackSlot, WritebackSlotRole};

const PARAGRAPH_SEPARATOR: &str = "\n\n";
const SENTENCE_BOUNDARIES: [char; 8] = ['。', '！', '？', '；', '!', '?', ';', '.'];
const CLAUSE_BOUNDARIES: [char; 10] = ['。', '！', '？', '；', '!', '?', ';', '.', '，', ','];
const CLOSING_PUNCTUATION: [char; 13] = [
    '"', '\'', '”', '’', '）', ')', '】', ']', '}', '」', '』', '》', '〉',
];

pub(crate) fn build_rewrite_units(
    slots: &[WritebackSlot],
    preset: SegmentationPreset,
) -> Vec<RewriteUnit> {
    let mut units = Vec::new();
    let mut current: Vec<&WritebackSlot> = Vec::new();

    for slot in slots {
        current.push(slot);
        if !should_close_unit(&current, preset) {
            continue;
        }
        if should_skip_unit(&current) {
            current.clear();
            continue;
        }
        units.push(build_unit(units.len(), preset, &current));
        current.clear();
    }

    if !current.is_empty() {
        if !should_skip_unit(&current) {
            units.push(build_unit(units.len(), preset, &current));
        }
    }

    units
}

fn should_skip_unit(current: &[&WritebackSlot]) -> bool {
    is_standalone_separator_unit(current) || is_blank_locked_unit(current)
}

fn build_unit(order: usize, preset: SegmentationPreset, slots: &[&WritebackSlot]) -> RewriteUnit {
    RewriteUnit {
        id: format!("unit-{order}"),
        order,
        slot_ids: slots.iter().map(|slot| slot.id.clone()).collect(),
        display_text: display_text(slots),
        segmentation_preset: preset,
        status: if slots.iter().any(|slot| slot.editable) {
            RewriteUnitStatus::Idle
        } else {
            RewriteUnitStatus::Done
        },
        error_message: None,
    }
}

fn display_text(slots: &[&WritebackSlot]) -> String {
    slots
        .iter()
        .map(|slot| format!("{}{}", slot.text, slot.separator_after))
        .collect()
}

fn should_close_unit(current: &[&WritebackSlot], preset: SegmentationPreset) -> bool {
    let Some(last) = current.last() else {
        return false;
    };
    if last.role == WritebackSlotRole::ParagraphBreak
        || last.separator_after.contains(PARAGRAPH_SEPARATOR)
    {
        return true;
    }
    if preset == SegmentationPreset::Paragraph {
        return false;
    }
    ends_semantic_group(current, preset)
}

fn is_standalone_separator_unit(current: &[&WritebackSlot]) -> bool {
    current.len() == 1
        && current[0].role == WritebackSlotRole::ParagraphBreak
        && current[0].text.is_empty()
}

fn is_blank_locked_unit(current: &[&WritebackSlot]) -> bool {
    current.iter().all(|slot| !slot.editable) && display_text(current).trim().is_empty()
}

fn ends_semantic_group(current: &[&WritebackSlot], preset: SegmentationPreset) -> bool {
    let text = display_text(current);
    let mut chars = text.chars().collect::<Vec<_>>();
    while chars.last().is_some_and(|ch| ch.is_whitespace()) {
        chars.pop();
    }
    while chars
        .last()
        .is_some_and(|ch| CLOSING_PUNCTUATION.contains(ch))
    {
        chars.pop();
    }
    let Some(last) = chars.last() else {
        return false;
    };
    match preset {
        SegmentationPreset::Clause => CLAUSE_BOUNDARIES.contains(last),
        SegmentationPreset::Sentence => SENTENCE_BOUNDARIES.contains(last),
        SegmentationPreset::Paragraph => false,
    }
}

#[cfg(test)]
mod tests {
    use crate::models::SegmentationPreset;

    use super::{build_rewrite_units, WritebackSlot};

    #[test]
    fn merges_adjacent_editable_slots_into_one_sentence_unit_when_no_boundary_exists() {
        let slots = vec![
            WritebackSlot::editable("slot-1", 0, "甲"),
            WritebackSlot::editable("slot-2", 1, "乙"),
        ];

        let units = build_rewrite_units(&slots, SegmentationPreset::Sentence);

        assert_eq!(units.len(), 1);
        assert_eq!(units[0].slot_ids, vec!["slot-1", "slot-2"]);
        assert_eq!(units[0].display_text, "甲乙");
    }

    #[test]
    fn paragraph_builder_skips_standalone_unit_for_empty_paragraph_break_slot() {
        let mut first = WritebackSlot::editable("slot-1", 0, "封面标题");
        first.separator_after = "\n\n".to_string();
        let mut empty_break = WritebackSlot::locked("slot-2", 1, "");
        empty_break.role = crate::rewrite_unit::WritebackSlotRole::ParagraphBreak;
        empty_break.separator_after = "\n\n".to_string();
        let second = WritebackSlot::editable("slot-3", 2, "正文开始");

        let units = build_rewrite_units(&[first, empty_break, second], SegmentationPreset::Paragraph);

        assert_eq!(units.len(), 2);
        assert_eq!(units[0].slot_ids, vec!["slot-1"]);
        assert_eq!(units[0].display_text, "封面标题\n\n");
        assert_eq!(units[1].slot_ids, vec!["slot-3"]);
        assert_eq!(units[1].display_text, "正文开始");
    }

    #[test]
    fn paragraph_builder_skips_blank_locked_whitespace_unit() {
        let mut blank = WritebackSlot::locked("slot-1", 0, "　　");
        blank.separator_after = "\n\n".to_string();
        let next = WritebackSlot::editable("slot-2", 1, "正文开始");

        let units = build_rewrite_units(&[blank, next], SegmentationPreset::Paragraph);

        assert_eq!(units.len(), 1);
        assert_eq!(units[0].slot_ids, vec!["slot-2"]);
        assert_eq!(units[0].display_text, "正文开始");
    }
}
