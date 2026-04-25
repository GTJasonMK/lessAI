use std::cell::{Cell, RefCell};

use chrono::Utc;

use crate::{
    models::{DocumentSession, RunningState, SegmentationPreset},
    persist,
    session_access::{load_session_for_source, SessionLoadSource},
    session_flow::{allow_session, run_session_steps, SessionStepConfig},
};

fn sample_session() -> DocumentSession {
    let now = Utc::now();
    let mut session = DocumentSession {
        id: "session-1".to_string(),
        title: "示例".to_string(),
        document_path: "/tmp/example.txt".to_string(),
        source_text: "正文".to_string(),
        source_snapshot: None,
        template_kind: None,
        template_signature: None,
        slot_structure_signature: None,
        template_snapshot: None,
        normalized_text: "正文".to_string(),
        capabilities: crate::session_capability_models::DocumentSessionCapabilities {
            source_writeback: crate::session_capability_models::CapabilityGate::allowed(),
            editor_writeback: crate::session_capability_models::CapabilityGate::allowed(),
            ..Default::default()
        },
        segmentation_preset: Some(SegmentationPreset::Paragraph),
        rewrite_headings: Some(false),
        writeback_slots: Vec::new(),
        rewrite_units: Vec::new(),
        suggestions: Vec::new(),
        next_suggestion_sequence: 1,
        status: RunningState::Idle,
        created_at: now,
        updated_at: now,
    };
    crate::documents::hydrate_session_capabilities(&mut session);
    session
}

fn run_mutation_steps<T, Guard, Load, Refresh, Mutate, Save>(
    source: SessionLoadSource<Guard>,
    load: Load,
    refresh: Refresh,
    mutate: Mutate,
    save: Save,
) -> Result<T, String>
where
    Load: FnOnce() -> Result<DocumentSession, String>,
    Guard: FnOnce(&DocumentSession) -> Result<(), String>,
    Refresh: FnOnce(DocumentSession) -> Result<DocumentSession, String>,
    Mutate: FnOnce(&mut DocumentSession) -> Result<super::SessionMutation<T>, String>,
    Save: FnOnce(&DocumentSession) -> Result<(), String>,
{
    run_session_steps(
        || load_session_for_source(source, load, refresh),
        SessionStepConfig::new(allow_session),
        |mut session| {
            let (value, should_save) = mutate(&mut session)?.into_parts();
            persist::maybe_save_and_return(value, should_save, |_| save(&session))
        },
    )
}

#[test]
fn session_mutation_save_marks_session_for_persist() {
    let mut session = sample_session();
    let now = Utc::now();

    let mutation = super::SessionMutation::save(&mut session, now, "ok".to_string());

    match mutation {
        super::SessionMutation::Save(value) => assert_eq!(value, "ok"),
        super::SessionMutation::SkipSave(_) => panic!("expected save mutation"),
    }
    assert_eq!(session.updated_at, now);
}

#[test]
fn mutate_loaded_session_for_stored_source_loads_then_saves() {
    let calls = RefCell::new(Vec::new());

    let value = run_mutation_steps(
        SessionLoadSource::stored(),
        || {
            calls.borrow_mut().push("load".to_string());
            Ok(sample_session())
        },
        Ok,
        |session| {
            calls.borrow_mut().push(format!("mutate:{}", session.id));
            session.title = "已修改".to_string();
            Ok(super::SessionMutation::Save(session.title.clone()))
        },
        |session| {
            calls.borrow_mut().push(format!("save:{}", session.title));
            Ok(())
        },
    )
    .expect("expected loaded mutation to succeed");

    assert_eq!(value, "已修改");
    assert_eq!(
        calls.into_inner(),
        vec![
            "load".to_string(),
            "mutate:session-1".to_string(),
            "save:已修改".to_string(),
        ]
    );
}

#[test]
fn mutate_loaded_session_for_stored_source_returns_load_error_before_mutate() {
    let mutate_calls = Cell::new(0);
    let save_calls = Cell::new(0);

    let error = run_mutation_steps(
        SessionLoadSource::stored(),
        || Err("load failed".to_string()),
        Ok,
        |_| {
            mutate_calls.set(mutate_calls.get() + 1);
            Ok(super::SessionMutation::Save("unused".to_string()))
        },
        |_| {
            save_calls.set(save_calls.get() + 1);
            Ok(())
        },
    )
    .expect_err("expected load failure to bubble up");

    assert_eq!(error, "load failed");
    assert_eq!(mutate_calls.get(), 0);
    assert_eq!(save_calls.get(), 0);
}

#[test]
fn mutate_loaded_session_for_stored_source_returns_mutate_error_before_save() {
    let save_calls = Cell::new(0);

    let error = run_mutation_steps(
        SessionLoadSource::stored(),
        || Ok(sample_session()),
        Ok,
        |_| Err::<super::SessionMutation<String>, String>("mutate failed".to_string()),
        |_| {
            save_calls.set(save_calls.get() + 1);
            Ok(())
        },
    )
    .expect_err("expected mutate failure to bubble up");

    assert_eq!(error, "mutate failed");
    assert_eq!(save_calls.get(), 0);
}

#[test]
fn mutate_current_session_steps_checks_idle_before_loading() {
    let load_calls = Cell::new(0);
    let mutate_calls = Cell::new(0);
    let save_calls = Cell::new(0);

    let error = (|| -> Result<(), String> {
        Err::<(), String>("busy".to_string())?;
        run_mutation_steps(
            SessionLoadSource::stored(),
            || {
                load_calls.set(load_calls.get() + 1);
                Ok(sample_session())
            },
            Ok,
            |_| {
                mutate_calls.set(mutate_calls.get() + 1);
                Ok(super::SessionMutation::Save("unused".to_string()))
            },
            |_| {
                save_calls.set(save_calls.get() + 1);
                Ok(())
            },
        )?;
        Ok(())
    })()
    .expect_err("expected active-job gate to short-circuit mutation");

    assert_eq!(error, "busy");
    assert_eq!(load_calls.get(), 0);
    assert_eq!(mutate_calls.get(), 0);
    assert_eq!(save_calls.get(), 0);
}

#[test]
fn mutate_loaded_session_for_source_refreshes_before_mutate() {
    let calls = RefCell::new(Vec::new());
    let save_calls = Cell::new(0);

    let value = run_mutation_steps(
        SessionLoadSource::refreshed(|session: &DocumentSession| {
            calls.borrow_mut().push(format!("guard:{}", session.id));
            Ok(())
        }),
        || {
            calls.borrow_mut().push("load".to_string());
            Ok(sample_session())
        },
        |mut session| {
            calls.borrow_mut().push(format!("refresh:{}", session.id));
            session.title = "刷新后".to_string();
            Ok(session)
        },
        |session| {
            calls.borrow_mut().push(format!("mutate:{}", session.title));
            Ok(super::SessionMutation::SkipSave(session.title.clone()))
        },
        |_| {
            save_calls.set(save_calls.get() + 1);
            Ok(())
        },
    )
    .expect("expected refreshed mutation helper to refresh before mutate");

    assert_eq!(value, "刷新后");
    assert_eq!(save_calls.get(), 0);
    assert_eq!(
        calls.into_inner(),
        vec![
            "load".to_string(),
            "refresh:session-1".to_string(),
            "guard:session-1".to_string(),
            "mutate:刷新后".to_string(),
        ]
    );
}

#[test]
fn session_load_request_exposes_constructor_signatures_for_mutation_chain() {
    let _ = crate::session_access::CurrentSessionRequest::stored;
    let _ = crate::session_access::CurrentSessionRequest::<
        fn(&DocumentSession) -> Result<(), String>,
    >::guarded_refresh;
}
