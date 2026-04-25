use crate::{
    models::{DocumentSession, DocumentSnapshot, SegmentationPreset},
    rewrite_unit::{RewriteUnit, WritebackSlot},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SegmentationRefreshAction {
    Keep,
    Rebuild,
    Block,
}

#[derive(Clone, Copy)]
pub(super) struct SegmentationRefreshExpectation<'a> {
    pub(super) expected_template_kind: Option<&'a str>,
    pub(super) expected_template_signature: Option<&'a str>,
    pub(super) expected_slot_structure_signature: Option<&'a str>,
    pub(super) expected_writeback_slots: &'a [WritebackSlot],
    pub(super) expected_rewrite_units: &'a [RewriteUnit],
    pub(super) segmentation_preset: SegmentationPreset,
    pub(super) rewrite_headings: bool,
}

pub(super) fn source_snapshot_changed(
    existing: &DocumentSession,
    current_snapshot: Option<&DocumentSnapshot>,
) -> bool {
    existing.source_snapshot.as_ref() != current_snapshot
}

pub(super) fn session_can_rebuild_cleanly(session: &DocumentSession) -> bool {
    session.capabilities.clean_session
}

pub(super) fn decide_segmentation_refresh(
    session: &DocumentSession,
    expected: SegmentationRefreshExpectation<'_>,
) -> SegmentationRefreshAction {
    if should_rebuild_structure(session, expected) {
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
    expected: SegmentationRefreshExpectation<'_>,
) -> bool {
    let template_kind_mismatch = !template_kind_compatible(
        session.template_kind.as_deref(),
        expected.expected_template_kind,
        session.document_path.as_str(),
    );
    session.segmentation_preset != Some(expected.segmentation_preset)
        || session.rewrite_headings != Some(expected.rewrite_headings)
        || template_kind_mismatch
        || session.template_signature.as_deref() != expected.expected_template_signature
        || session.slot_structure_signature.as_deref() != expected.expected_slot_structure_signature
        || !writeback_slot_structures_match(
            &session.writeback_slots,
            expected.expected_writeback_slots,
        )
        || !rewrite_unit_structures_match(&session.rewrite_units, expected.expected_rewrite_units)
}

fn template_kind_compatible(
    current: Option<&str>,
    expected: Option<&str>,
    document_path: &str,
) -> bool {
    if current == expected {
        return true;
    }
    if expected == Some("docx")
        && current.is_none()
        && document_path
            .rsplit_once('.')
            .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("docx"))
    {
        // Backward compatibility: old docx sessions persisted template_kind=None.
        return true;
    }
    false
}

fn writeback_slot_structures_match(current: &[WritebackSlot], expected: &[WritebackSlot]) -> bool {
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

#[cfg(test)]
mod tests {
    #[test]
    fn template_kind_compatible_accepts_legacy_docx_none() {
        assert!(super::template_kind_compatible(
            None,
            Some("docx"),
            "/tmp/example.docx"
        ));
    }

    #[test]
    fn template_kind_compatible_rejects_non_docx_none() {
        assert!(!super::template_kind_compatible(
            None,
            Some("markdown"),
            "/tmp/example.md"
        ));
    }
}
