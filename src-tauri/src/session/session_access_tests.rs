use std::cell::{Cell, RefCell};

use crate::{
    models::{DocumentSession, RunningState},
    session_flow::{run_session_steps, SessionStepConfig},
    test_support::sample_clean_session,
};

fn sample_session(id: &str, title: &str) -> DocumentSession {
    let mut session = sample_clean_session(id, "/tmp/example.txt", "正文");
    session.title = title.to_string();
    session
}

fn run_access_steps<T, EnsureIdle, Load, Run>(
    session_id: &str,
    ensure_idle: EnsureIdle,
    load: Load,
    active_job_error: Option<&str>,
    run: Run,
) -> Result<T, String>
where
    EnsureIdle: FnOnce(&str) -> Result<(), String>,
    Load: FnOnce(&str) -> Result<DocumentSession, String>,
    Run: FnOnce(DocumentSession) -> Result<T, String>,
{
    ensure_idle(session_id)?;
    run_session_steps(
        || load(session_id),
        SessionStepConfig::new(move |session: &DocumentSession| {
            super::ensure_loaded_session_is_idle(session_id, session, active_job_error)
        }),
        run,
    )
}

#[test]
fn load_session_then_refresh_runs_refresh_before_guard() {
    let calls = RefCell::new(Vec::new());

    let session = super::load_session_for_source(
        super::SessionLoadSource::refreshed(|session: &DocumentSession| {
            calls.borrow_mut().push(format!("guard:{}", session.id));
            Ok(())
        }),
        || {
            calls.borrow_mut().push("load".to_string());
            Ok(sample_session("session-0", "loaded"))
        },
        |session| {
            calls.borrow_mut().push(format!("refresh:{}", session.id));
            Ok(session)
        },
    )
    .expect("expected helper to load, guard, and refresh");

    assert_eq!(session.id, "session-0");
    assert_eq!(
        calls.into_inner(),
        vec![
            "load".to_string(),
            "refresh:session-0".to_string(),
            "guard:session-0".to_string(),
        ]
    );
}

#[test]
fn load_session_then_refresh_returns_guard_error_after_refresh() {
    let refresh_calls = Cell::new(0);

    let error = super::load_session_for_source(
        super::SessionLoadSource::refreshed(|_: &DocumentSession| {
            Err::<(), String>("guard failed".to_string())
        }),
        || Ok(sample_session("session-0", "loaded")),
        |_| {
            refresh_calls.set(refresh_calls.get() + 1);
            Ok(sample_session("session-0", "refreshed"))
        },
    )
    .expect_err("expected guard failure after refreshed session is loaded");

    assert_eq!(error, "guard failed");
    assert_eq!(refresh_calls.get(), 1);
}

#[test]
fn access_current_session_steps_loads_then_runs() {
    let calls = RefCell::new(Vec::new());

    let session = run_access_steps(
        "session-3",
        |_| Ok(()),
        |session_id| {
            calls.borrow_mut().push(format!("load:{session_id}"));
            Ok(sample_session(session_id, "loaded"))
        },
        None,
        |session| {
            calls.borrow_mut().push(format!("run:{}", session.id));
            Ok(session)
        },
    )
    .expect("expected helper to succeed");

    assert_eq!(session.id, "session-3");
    assert_eq!(
        calls.into_inner(),
        vec!["load:session-3".to_string(), "run:session-3".to_string()]
    );
}

#[test]
fn access_current_session_steps_returns_load_error() {
    let run_calls = Cell::new(0);

    let error = run_access_steps(
        "session-4",
        |_| Ok(()),
        |_| Err("load failed".to_string()),
        None,
        |_| {
            run_calls.set(run_calls.get() + 1);
            Ok(())
        },
    )
    .expect_err("expected load failure to bubble up");

    assert_eq!(error, "load failed");
    assert_eq!(run_calls.get(), 0);
}

#[test]
fn access_current_session_steps_returns_run_error() {
    let error = run_access_steps(
        "session-5",
        |_| Ok(()),
        |session_id| Ok(sample_session(session_id, "loaded")),
        None,
        |_| Err::<(), String>("run failed".to_string()),
    )
    .expect_err("expected run failure to bubble up");

    assert_eq!(error, "run failed");
}

#[test]
fn access_current_session_steps_checks_idle_before_loading() {
    let calls = RefCell::new(Vec::new());

    let session = run_access_steps(
        "session-10",
        |session_id| {
            calls.borrow_mut().push(format!("ensure:{session_id}"));
            Ok(())
        },
        |session_id| {
            calls.borrow_mut().push(format!("load:{session_id}"));
            Ok(sample_session(session_id, "loaded"))
        },
        Some("busy"),
        |session| {
            calls.borrow_mut().push(format!("run:{}", session.id));
            Ok(session)
        },
    )
    .expect("expected helper to succeed");

    assert_eq!(session.id, "session-10");
    assert_eq!(
        calls.into_inner(),
        vec![
            "ensure:session-10".to_string(),
            "load:session-10".to_string(),
            "run:session-10".to_string(),
        ]
    );
}

#[test]
fn access_current_session_steps_stops_before_loading_when_job_exists() {
    let load_calls = Cell::new(0);
    let run_calls = Cell::new(0);

    let error = run_access_steps(
        "session-11",
        |_| Err("busy".to_string()),
        |_| {
            load_calls.set(load_calls.get() + 1);
            Ok(sample_session("session-11", "loaded"))
        },
        Some("busy"),
        |_| {
            run_calls.set(run_calls.get() + 1);
            Ok(())
        },
    )
    .expect_err("expected helper to short-circuit before load");

    assert_eq!(error, "busy");
    assert_eq!(load_calls.get(), 0);
    assert_eq!(run_calls.get(), 0);
}

#[test]
fn access_current_session_steps_rejects_loaded_session_that_is_still_active() {
    let run_calls = Cell::new(0);

    let error = run_access_steps(
        "session-12",
        |_| Ok(()),
        |session_id| {
            let mut session = sample_session(session_id, "active");
            session.status = RunningState::Running;
            Ok(session)
        },
        Some("still active"),
        |_| {
            run_calls.set(run_calls.get() + 1);
            Ok(())
        },
    )
    .expect_err("expected active loaded session to be rejected");

    assert_eq!(error, "still active");
    assert_eq!(run_calls.get(), 0);
}

#[test]
fn access_current_session_steps_returns_run_error_after_loading() {
    let load_calls = Cell::new(0);

    let error = run_access_steps(
        "session-13",
        |_| Ok(()),
        |session_id| {
            load_calls.set(load_calls.get() + 1);
            Ok(sample_session(session_id, "loaded"))
        },
        Some("busy"),
        |_| Err::<(), String>("run failed".to_string()),
    )
    .expect_err("expected run failure to bubble up");

    assert_eq!(error, "run failed");
    assert_eq!(load_calls.get(), 1);
}

#[test]
fn access_current_session_returns_run_error_from_idle_guarded_path() {
    let error = run_access_steps(
        "session-14",
        |_| Ok(()),
        |session_id| Ok(sample_session(session_id, "loaded")),
        Some("busy"),
        |_| Err::<DocumentSession, String>("blocked".to_string()),
    )
    .expect_err("expected validation error to be returned");

    assert_eq!(error, "blocked");
}

#[test]
fn access_current_session_steps_loads_then_mutates_session() {
    let calls = RefCell::new(Vec::new());

    let title = run_access_steps(
        "session-15",
        |session_id| {
            calls.borrow_mut().push(format!("ensure:{session_id}"));
            Ok(())
        },
        |session_id| {
            calls.borrow_mut().push(format!("load:{session_id}"));
            Ok(sample_session(session_id, "loaded"))
        },
        Some("busy"),
        |mut session| {
            calls.borrow_mut().push(format!("edit:{}", session.id));
            session.title = "edited".to_string();
            Ok(session.title.clone())
        },
    )
    .expect("expected edit helper to mutate loaded session");

    assert_eq!(title, "edited");
    assert_eq!(
        calls.into_inner(),
        vec![
            "ensure:session-15".to_string(),
            "load:session-15".to_string(),
            "edit:session-15".to_string(),
        ]
    );
}

#[test]
fn current_session_request_exposes_constructor_signatures() {
    let _ = super::CurrentSessionRequest::stored;
    let _ =
        super::CurrentSessionRequest::<fn(&DocumentSession) -> Result<(), String>>::guarded_refresh;
}

#[test]
fn current_session_request_exposes_active_job_error_builder_signature() {
    let _ = super::CurrentSessionRequest::<fn(&DocumentSession) -> Result<(), String>>::with_active_job_error;
}

#[test]
fn load_session_for_source_returns_stored_session_without_refresh() {
    let calls = RefCell::new(Vec::new());

    let session = super::load_session_for_source(
        super::SessionLoadSource::stored(),
        || {
            calls.borrow_mut().push("load".to_string());
            Ok(sample_session("session-15", "stored"))
        },
        |session| {
            calls.borrow_mut().push(format!("refresh:{}", session.id));
            Ok(session)
        },
    )
    .expect("expected stored source to skip refresh");

    assert_eq!(session.id, "session-15");
    assert_eq!(calls.into_inner(), vec!["load".to_string()]);
}

#[test]
fn load_session_for_source_runs_refresh_before_guard() {
    let calls = RefCell::new(Vec::new());

    let session = super::load_session_for_source(
        super::SessionLoadSource::refreshed(|session: &DocumentSession| {
            calls.borrow_mut().push(format!("guard:{}", session.id));
            Ok(())
        }),
        || {
            calls.borrow_mut().push("load".to_string());
            Ok(sample_session("session-16", "refreshed"))
        },
        |session| {
            calls.borrow_mut().push(format!("refresh:{}", session.id));
            Ok(session)
        },
    )
    .expect("expected refreshed source to run guard then refresh");

    assert_eq!(session.id, "session-16");
    assert_eq!(
        calls.into_inner(),
        vec![
            "load".to_string(),
            "refresh:session-16".to_string(),
            "guard:session-16".to_string(),
        ]
    );
}

#[test]
fn repair_stale_active_session_steps_downgrades_session_without_live_job() {
    let save_calls = Cell::new(0);
    let mut active = sample_session("session-17", "active");
    active.status = RunningState::Running;

    let repaired = super::repair_stale_active_session_steps(
        active,
        Some("busy"),
        || Ok(false),
        |_| {
            save_calls.set(save_calls.get() + 1);
            Ok(())
        },
        chrono::Utc::now(),
    )
    .expect("expected stale active session to be downgraded");

    assert_eq!(repaired.status, RunningState::Cancelled);
    assert_eq!(save_calls.get(), 1);
}

#[test]
fn repair_stale_active_session_steps_keeps_live_active_session_unchanged() {
    let save_calls = Cell::new(0);
    let mut active = sample_session("session-18", "active");
    active.status = RunningState::Paused;

    let repaired = super::repair_stale_active_session_steps(
        active,
        Some("busy"),
        || Ok(true),
        |_| {
            save_calls.set(save_calls.get() + 1);
            Ok(())
        },
        chrono::Utc::now(),
    )
    .expect("expected live active session to stay untouched");

    assert_eq!(repaired.status, RunningState::Paused);
    assert_eq!(save_calls.get(), 0);
}
