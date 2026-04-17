use chrono::Utc;

use crate::{
    models::{SegmentationPreset, RewriteUnitStatus, DocumentSession, RunningState},
    rewrite_batch_commit::{batch_commit_mode, rewrite_unit_completed_events},
    rewrite_unit::{build_rewrite_units, WritebackSlot},
    test_support::{editable_slot, locked_slot},
};

fn session_with_slots_and_units(slots: Vec<WritebackSlot>) -> DocumentSession {
    let now = Utc::now();
    let source_text = slots
        .iter()
        .map(|slot| format!("{}{}", slot.text, slot.separator_after))
        .collect::<String>();
    DocumentSession {
        id: "session-1".to_string(),
        title: "示例".to_string(),
        document_path: "/tmp/example.txt".to_string(),
        source_text: source_text.clone(),
        source_snapshot: None,
        normalized_text: source_text,
        write_back_supported: true,
        write_back_block_reason: None,
        plain_text_editor_safe: true,
        plain_text_editor_block_reason: None,
        segmentation_preset: Some(SegmentationPreset::Paragraph),
        rewrite_headings: Some(false),
        rewrite_units: build_rewrite_units(&slots, SegmentationPreset::Paragraph),
        writeback_slots: slots,
        suggestions: Vec::new(),
        next_suggestion_sequence: 1,
        status: RunningState::Idle,
        created_at: now,
        updated_at: now,
    }
}

fn session_with_unit_statuses(statuses: &[RewriteUnitStatus]) -> DocumentSession {
    let slots = statuses
        .iter()
        .enumerate()
        .map(|(index, _)| {
            let mut slot = editable_slot(&format!("slot-{index}"), index, &format!("chunk-{index}"));
            if index + 1 < statuses.len() {
                slot.separator_after = "\n\n".to_string();
            }
            slot
        })
        .collect::<Vec<_>>();
    let mut session = session_with_slots_and_units(slots);
    for (unit, status) in session.rewrite_units.iter_mut().zip(statuses.iter().copied()) {
        unit.status = status;
        unit.error_message = Some("旧错误".to_string());
    }
    session
}

#[test]
fn clear_running_units_resets_only_running_units() {
    let mut session = session_with_unit_statuses(&[
        RewriteUnitStatus::Running,
        RewriteUnitStatus::Failed,
        RewriteUnitStatus::Done,
    ]);

    let touched = crate::rewrite_job_state::clear_running_units(&mut session);

    assert!(touched);
    assert_eq!(session.rewrite_units[0].status, RewriteUnitStatus::Idle);
    assert_eq!(session.rewrite_units[0].error_message, None);
    assert_eq!(session.rewrite_units[1].status, RewriteUnitStatus::Failed);
    assert_eq!(
        session.rewrite_units[1].error_message.as_deref(),
        Some("旧错误")
    );
}

#[test]
fn fail_running_units_marks_only_running_units() {
    let mut session = session_with_unit_statuses(&[
        RewriteUnitStatus::Running,
        RewriteUnitStatus::Idle,
        RewriteUnitStatus::Done,
    ]);

    let touched = crate::rewrite_job_state::fail_running_units(&mut session, "失败原因");

    assert!(touched);
    assert_eq!(session.rewrite_units[0].status, RewriteUnitStatus::Failed);
    assert_eq!(
        session.rewrite_units[0].error_message.as_deref(),
        Some("失败原因")
    );
    assert_eq!(session.rewrite_units[1].status, RewriteUnitStatus::Idle);
}

#[test]
fn set_units_running_status_preserves_paused_session_state() {
    let mut session = session_with_unit_statuses(&[RewriteUnitStatus::Idle, RewriteUnitStatus::Idle]);
    session.status = RunningState::Paused;
    let target = vec!["unit-0".to_string()];

    crate::rewrite_job_state::set_units_running_status(&mut session, &target)
        .expect("set running status");

    assert_eq!(session.status, RunningState::Paused);
    assert_eq!(session.rewrite_units[0].status, RewriteUnitStatus::Running);
    assert_eq!(session.rewrite_units[1].status, RewriteUnitStatus::Idle);
}

#[test]
fn update_target_units_rejects_unknown_without_partial_mutation() {
    let mut session = session_with_unit_statuses(&[RewriteUnitStatus::Idle, RewriteUnitStatus::Idle]);
    let before = session.rewrite_units.clone();
    let target = vec!["unit-x".to_string()];

    let error = crate::rewrite_job_state::update_target_units(
        &mut session,
        &target,
        RewriteUnitStatus::Running,
        None,
    )
    .expect_err("unknown unit should fail");

    assert_eq!(error, crate::rewrite_permissions::REWRITE_UNIT_NOT_FOUND_ERROR);
    assert_eq!(session.rewrite_units, before);
}

#[test]
fn resolve_available_targets_and_manual_batch_return_selected_idle_or_failed_units() {
    let mut session = session_with_unit_statuses(&[
        RewriteUnitStatus::Idle,
        RewriteUnitStatus::Failed,
        RewriteUnitStatus::Done,
    ]);
    session.rewrite_units[0].id = "unit-0".to_string();
    session.rewrite_units[1].id = "unit-1".to_string();
    session.rewrite_units[2].id = "unit-2".to_string();

    let targets = super::resolve_available_rewrite_targets(
        &session,
        Some(vec!["unit-1".to_string(), "unit-2".to_string()]),
    )
    .expect("target resolution should succeed");
    let batch = crate::rewrite_targets::find_next_manual_batch(
        &session.rewrite_units,
        targets.target_unit_ids.as_ref(),
        2,
    );
    let batch = super::ensure_targets_available(batch, targets.has_target_subset, Vec::is_empty)
        .expect("manual batch should resolve");

    assert_eq!(batch, vec!["unit-1".to_string()]);
}

#[test]
fn collect_rewrite_batch_source_texts_rejects_locked_only_unit() {
    let mut locked = locked_slot("slot-0", 0, "[公式]");
    locked.separator_after = "\n\n".to_string();
    let session = session_with_slots_and_units(vec![locked, editable_slot("slot-1", 1, "正文")]);
    let snapshot = super::build_rewrite_source_snapshot(&session).expect("snapshot");

    let error = super::collect_rewrite_batch_source_texts(
        &snapshot,
        &["unit-0".to_string()],
    )
    .expect_err("locked-only unit should fail");

    assert!(error.contains("保护区"));
}

#[test]
fn collect_rewrite_batch_source_texts_returns_requests_in_batch_order() {
    let mut first = editable_slot("slot-0", 0, "第一段");
    first.separator_after = "\n\n".to_string();
    let session = session_with_slots_and_units(vec![first, editable_slot("slot-1", 1, "第二段")]);
    let snapshot = super::build_rewrite_source_snapshot(&session).expect("snapshot");

    let requests = super::collect_rewrite_batch_source_texts(
        &snapshot,
        &["unit-1".to_string(), "unit-0".to_string()],
    )
    .expect("collect source texts");

    let ids = requests
        .iter()
        .map(|request| request.rewrite_unit_id.clone())
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["unit-1".to_string(), "unit-0".to_string()]);
}

#[test]
fn prepare_loaded_rewrite_batch_builds_single_batch_request() {
    let mut first = editable_slot("slot-0", 0, "第一段");
    first.separator_after = "\n\n".to_string();
    let session = session_with_slots_and_units(vec![first, editable_slot("slot-1", 1, "第二段")]);

    let prepared = super::prepare_loaded_rewrite_batch(
        &session,
        &["unit-0".to_string(), "unit-1".to_string()],
    )
    .expect("batch should prepare");

    assert_eq!(
        prepared.rewrite_unit_ids,
        vec!["unit-0".to_string(), "unit-1".to_string()]
    );
    assert_eq!(prepared.batch_request.units.len(), 2);
    assert_eq!(prepared.batch_request.units[0].rewrite_unit_id, "unit-0");
    assert_eq!(prepared.batch_request.units[1].rewrite_unit_id, "unit-1");
}

#[test]
fn rewrite_unit_completed_events_preserve_batch_order_and_session_id() {
    let events = rewrite_unit_completed_events(
        "session-1",
        &[
            ("unit-2".to_string(), "suggestion-2".to_string(), 2),
            ("unit-0".to_string(), "suggestion-1".to_string(), 1),
        ],
    );

    assert_eq!(events[0].session_id, "session-1");
    assert_eq!(events[0].rewrite_unit_id, "unit-2");
    assert_eq!(events[1].rewrite_unit_id, "unit-0");
}

#[test]
fn batch_commit_mode_marks_auto_approve_as_applied() {
    let approved = batch_commit_mode(true);
    let proposed = batch_commit_mode(false);

    assert_eq!(approved.decision, crate::models::SuggestionDecision::Applied);
    assert_eq!(approved.set_status, None);
    assert_eq!(proposed.decision, crate::models::SuggestionDecision::Proposed);
    assert_eq!(proposed.set_status, Some(RunningState::Idle));
}
