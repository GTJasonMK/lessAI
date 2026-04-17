use std::{collections::HashSet, sync::Arc};

use chrono::Utc;

use crate::{
    models::{SegmentationPreset, RewriteUnitStatus, DocumentSession, RunningState},
    rewrite_unit::RewriteUnit,
    state::JobControl,
};

fn sample_session() -> DocumentSession {
    let now = Utc::now();
    DocumentSession {
        id: "session-auto".to_string(),
        title: "示例".to_string(),
        document_path: "/tmp/example.txt".to_string(),
        source_text: "正文".to_string(),
        source_snapshot: None,
        normalized_text: "正文".to_string(),
        write_back_supported: true,
        write_back_block_reason: None,
        plain_text_editor_safe: true,
        plain_text_editor_block_reason: None,
        segmentation_preset: Some(SegmentationPreset::Paragraph),
        rewrite_headings: Some(false),
        writeback_slots: Vec::new(),
        rewrite_units: vec![RewriteUnit {
            id: "unit-0".to_string(),
            order: 0,
            slot_ids: vec!["slot-0".to_string()],
            display_text: "正文".to_string(),
            segmentation_preset: SegmentationPreset::Paragraph,
            status: RewriteUnitStatus::Idle,
            error_message: None,
        }],
        suggestions: Vec::new(),
        next_suggestion_sequence: 1,
        status: RunningState::Idle,
        created_at: now,
        updated_at: now,
    }
}

#[test]
fn start_auto_rewrite_session_steps_marks_running_and_saves() {
    let mut session = sample_session();
    let reserved_job = Arc::new(JobControl::default());
    let target_indices = Some(HashSet::from(["unit-0".to_string()]));
    let now = Utc::now();
    let calls = std::cell::RefCell::new(Vec::new());

    let (saved_session, returned_targets, job) = super::start_auto_rewrite_session_steps(
        &mut session,
        target_indices.clone(),
        |session_id| {
            calls.borrow_mut().push(format!("reserve:{session_id}"));
            Ok(reserved_job.clone())
        },
        |saved_session| {
            calls
                .borrow_mut()
                .push(format!("save:{:?}", saved_session.status));
            Ok(())
        },
        |session_id| {
            calls.borrow_mut().push(format!("rollback:{session_id}"));
            Ok(())
        },
        now,
    )
    .expect("expected auto-start helper to reserve and save");

    assert!(Arc::ptr_eq(&job, &reserved_job));
    assert_eq!(returned_targets, target_indices);
    assert_eq!(session.status, RunningState::Running);
    assert_eq!(session.updated_at, now);
    assert_eq!(saved_session.status, RunningState::Running);
    assert_eq!(saved_session.updated_at, now);
    assert_eq!(
        calls.into_inner(),
        vec![
            "reserve:session-auto".to_string(),
            "save:Running".to_string(),
        ]
    );
}

#[test]
fn start_auto_rewrite_session_steps_returns_reserve_error_before_save() {
    let mut session = sample_session();
    let save_calls = std::cell::Cell::new(0);
    let rollback_calls = std::cell::Cell::new(0);

    let error = match super::start_auto_rewrite_session_steps(
        &mut session,
        None,
        |_| Err("reserve failed".to_string()),
        |_| {
            save_calls.set(save_calls.get() + 1);
            Ok(())
        },
        |_| {
            rollback_calls.set(rollback_calls.get() + 1);
            Ok(())
        },
        Utc::now(),
    ) {
        Ok(_) => panic!("expected reserve failure to bubble up"),
        Err(error) => error,
    };

    assert_eq!(error, "reserve failed");
    assert_eq!(save_calls.get(), 0);
    assert_eq!(rollback_calls.get(), 0);
}

#[test]
fn start_auto_rewrite_session_steps_rolls_back_when_save_fails() {
    let mut session = sample_session();
    let rollback_calls = std::cell::RefCell::new(Vec::new());

    let error = match super::start_auto_rewrite_session_steps(
        &mut session,
        None,
        |_| Ok(Arc::new(JobControl::default())),
        |_| Err("save failed".to_string()),
        |session_id| {
            rollback_calls.borrow_mut().push(session_id.to_string());
            Ok(())
        },
        Utc::now(),
    ) {
        Ok(_) => panic!("expected save failure to bubble up"),
        Err(error) => error,
    };

    assert_eq!(error, "save failed");
    assert_eq!(
        rollback_calls.into_inner(),
        vec!["session-auto".to_string()]
    );
}

#[test]
fn finish_spawned_auto_loop_steps_removes_job_before_finished_signal() {
    let calls = std::cell::RefCell::new(Vec::new());

    super::finish_spawned_auto_loop_steps(
        Ok(()),
        || {
            calls.borrow_mut().push("remove".to_string());
            Ok(())
        },
        || {
            calls.borrow_mut().push("finished".to_string());
            Ok(())
        },
        |error| {
            calls.borrow_mut().push(format!("failed:{error}"));
            Ok(())
        },
    )
    .expect("expected successful loop result to remove job before finished signal");

    assert_eq!(
        calls.into_inner(),
        vec!["remove".to_string(), "finished".to_string()]
    );
}

#[test]
fn finish_spawned_auto_loop_steps_removes_job_before_failed_signal() {
    let calls = std::cell::RefCell::new(Vec::new());

    super::finish_spawned_auto_loop_steps(
        Err("loop failed".to_string()),
        || {
            calls.borrow_mut().push("remove".to_string());
            Ok(())
        },
        || {
            calls.borrow_mut().push("finished".to_string());
            Ok(())
        },
        |error| {
            calls.borrow_mut().push(format!("failed:{error}"));
            Ok(())
        },
    )
    .expect("expected failed loop result to remove job before failed signal");

    assert_eq!(
        calls.into_inner(),
        vec!["remove".to_string(), "failed:loop failed".to_string()]
    );
}
