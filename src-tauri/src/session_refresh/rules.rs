use crate::{
    models::{SegmentationPreset, RewriteUnitStatus, DocumentSession, DocumentSnapshot, RunningState},
    rewrite_unit::{RewriteUnit, WritebackSlot},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SegmentationRefreshAction {
    Keep,
    Rebuild,
    Block,
}

pub(super) fn source_snapshot_changed(
    existing: &DocumentSession,
    current_snapshot: Option<&DocumentSnapshot>,
) -> bool {
    existing.source_snapshot.as_ref() != current_snapshot
}

pub(super) fn session_can_rebuild_cleanly(session: &DocumentSession) -> bool {
    session.status == RunningState::Idle
        && session.suggestions.is_empty()
        && session
            .rewrite_units
            .iter()
            .all(|unit| matches!(unit.status, RewriteUnitStatus::Idle | RewriteUnitStatus::Done))
}

pub(super) fn decide_segmentation_refresh(
    session: &DocumentSession,
    expected_writeback_slots: &[WritebackSlot],
    expected_rewrite_units: &[RewriteUnit],
    segmentation_preset: SegmentationPreset,
    rewrite_headings: bool,
) -> SegmentationRefreshAction {
    if should_rebuild_structure(
        session,
        expected_writeback_slots,
        expected_rewrite_units,
        segmentation_preset,
        rewrite_headings,
    ) {
        return if session_can_rebuild_cleanly(session) {
            SegmentationRefreshAction::Rebuild
        } else {
            SegmentationRefreshAction::Block
        };
    }

    SegmentationRefreshAction::Keep
}

fn should_rebuild_structure(
    session: &DocumentSession,
    expected_writeback_slots: &[WritebackSlot],
    expected_rewrite_units: &[RewriteUnit],
    segmentation_preset: SegmentationPreset,
    rewrite_headings: bool,
) -> bool {
    session.segmentation_preset != Some(segmentation_preset)
        || session.rewrite_headings != Some(rewrite_headings)
        || !writeback_slot_structures_match(&session.writeback_slots, expected_writeback_slots)
        || !rewrite_unit_structures_match(&session.rewrite_units, expected_rewrite_units)
}

fn writeback_slot_structures_match(
    current: &[WritebackSlot],
    expected: &[WritebackSlot],
) -> bool {
    current.len() == expected.len()
        && current.iter().zip(expected.iter()).all(|(left, right)| {
            left.order == right.order
                && left.text == right.text
                && left.editable == right.editable
                && left.role == right.role
                && left.presentation == right.presentation
                && left.anchor == right.anchor
                && left.separator_after == right.separator_after
        })
}

fn rewrite_unit_structures_match(current: &[RewriteUnit], expected: &[RewriteUnit]) -> bool {
    current.len() == expected.len()
        && current.iter().zip(expected.iter()).all(|(left, right)| {
            left.order == right.order
                && left.slot_ids == right.slot_ids
                && left.display_text == right.display_text
                && left.segmentation_preset == right.segmentation_preset
        })
}
