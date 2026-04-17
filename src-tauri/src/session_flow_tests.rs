use std::cell::{Cell, RefCell};

use chrono::Utc;

use crate::{
    models::{SegmentationPreset, DocumentSession, RunningState},
    persist,
    session_flow::{allow_session, SessionLock, SessionStepConfig},
};

fn sample_session(id: &str) -> DocumentSession {
    let now = Utc::now();
    DocumentSession {
        id: id.to_string(),
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
        rewrite_units: Vec::new(),
        suggestions: Vec::new(),
        next_suggestion_sequence: 1,
        status: RunningState::Idle,
        created_at: now,
        updated_at: now,
    }
}

#[test]
fn run_session_steps_runs_guard_before_apply() {
    let calls = RefCell::new(Vec::new());

    let session = super::run_session_steps(
        || {
            calls.borrow_mut().push("load".to_string());
            Ok(sample_session("session-1"))
        },
        SessionStepConfig::new(|session: &DocumentSession| {
            calls.borrow_mut().push(format!("guard:{}", session.id));
            Ok(())
        }),
        |session| {
            calls.borrow_mut().push(format!("apply:{}", session.id));
            Ok(session)
        },
    )
    .expect("expected guarded load to succeed");

    assert_eq!(session.id, "session-1");
    assert_eq!(
        calls.into_inner(),
        vec![
            "load".to_string(),
            "guard:session-1".to_string(),
            "apply:session-1".to_string(),
        ]
    );
}

#[test]
fn run_session_steps_honors_lock_and_short_circuits_failed_load() {
    let state = crate::state::AppState::default();
    let apply_calls = Cell::new(0);

    let error = super::run_session_steps(
        || Err("load failed".to_string()),
        SessionStepConfig::locked(SessionLock::new(&state, "session-2"), allow_session),
        |_| {
            apply_calls.set(apply_calls.get() + 1);
            Ok(())
        },
    )
    .expect_err("expected load failure to short-circuit");

    assert_eq!(error, "load failed");
    assert_eq!(apply_calls.get(), 0);
}

#[test]
fn refreshed_session_persists_only_when_changed() {
    let save_calls = Cell::new(0);
    let unchanged = sample_session("session-3");
    let changed = sample_session("session-4");

    let unchanged = persist::maybe_save_and_return(unchanged, false, |_| {
        save_calls.set(save_calls.get() + 1);
        Ok(())
    })
    .expect("expected unchanged refresh to skip save");
    let changed = persist::maybe_save_and_return(changed, true, |_| {
        save_calls.set(save_calls.get() + 1);
        Ok(())
    })
    .expect("expected changed refresh to persist");

    assert_eq!(unchanged.id, "session-3");
    assert_eq!(changed.id, "session-4");
    assert_eq!(save_calls.get(), 1);
}

#[test]
fn open_existing_or_clean_session_steps_refreshes_existing_session() {
    let calls = RefCell::new(Vec::new());

    let session = super::open_existing_or_clean_session_steps(
        || Ok(Some(sample_session("session-5"))),
        |_| {
            calls.borrow_mut().push("save".to_string());
            Ok(())
        },
        |session| {
            calls.borrow_mut().push(format!("refresh:{}", session.id));
            Ok(session)
        },
        || {
            calls.borrow_mut().push("clean".to_string());
            Ok(sample_session("unused"))
        },
        Utc::now(),
    )
    .expect("expected existing session to refresh");

    assert_eq!(session.id, "session-5");
    assert_eq!(calls.into_inner(), vec!["refresh:session-5".to_string()]);
}

#[test]
fn open_existing_or_clean_session_steps_loads_clean_when_missing() {
    let calls = RefCell::new(Vec::new());

    let session = super::open_existing_or_clean_session_steps(
        || Ok(None),
        |_| {
            calls.borrow_mut().push("save".to_string());
            Ok(())
        },
        |_| {
            panic!("expected missing session to skip refresh");
        },
        || {
            calls.borrow_mut().push("clean".to_string());
            Ok(sample_session("session-6"))
        },
        Utc::now(),
    )
    .expect("expected clean session to load");

    assert_eq!(session.id, "session-6");
    assert_eq!(
        calls.into_inner(),
        vec!["clean".to_string(), "save".to_string()]
    );
}
