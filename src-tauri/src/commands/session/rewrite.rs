use std::sync::atomic::Ordering;

use log::{error, info};
use tauri::{AppHandle, State};

use crate::{
    models::{DocumentSession, RewriteMode},
    observability::{rewrite_mode_label, target_rewrite_unit_ids_label},
    rewrite_job_state, rewrite_jobs,
    session_access::{access_current_session, CurrentSessionRequest},
    state::{load_job, require_job, AppState},
};

fn finish_rewrite_signal_steps<T, Mark, Signal>(mark: Mark, signal: Signal) -> Result<T, String>
where
    Mark: FnOnce() -> Result<T, String>,
    Signal: FnOnce(),
{
    let result = mark()?;
    signal();
    Ok(result)
}

#[tauri::command]
pub async fn start_rewrite(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    mode: RewriteMode,
    target_rewrite_unit_ids: Option<Vec<String>>,
) -> Result<DocumentSession, String> {
    let target_label = target_rewrite_unit_ids_label(target_rewrite_unit_ids.as_deref());
    info!(
        "rewrite requested: session_id={} mode={} target_rewrite_unit_ids={target_label}",
        session_id,
        rewrite_mode_label(mode),
    );

    let result = match mode {
        RewriteMode::Manual => {
            rewrite_jobs::run_manual_rewrite(
                &app,
                state.inner(),
                &session_id,
                target_rewrite_unit_ids,
            )
            .await
        }
        RewriteMode::Auto => {
            rewrite_jobs::run_auto_rewrite(app, state, &session_id, target_rewrite_unit_ids)
        }
    };

    match &result {
        Ok(_) => info!(
            "rewrite request accepted: session_id={} mode={} target_rewrite_unit_ids={target_label}",
            session_id,
            rewrite_mode_label(mode),
        ),
        Err(message) => error!(
            "rewrite request failed: session_id={} mode={} target_rewrite_unit_ids={target_label} error={message}",
            session_id,
            rewrite_mode_label(mode),
        ),
    }

    result
}

#[tauri::command]
pub fn pause_rewrite(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
) -> Result<DocumentSession, String> {
    let job = require_job(state.inner(), &session_id, "当前没有可暂停的任务。")?;

    finish_rewrite_signal_steps(
        || rewrite_job_state::mark_session_paused(&app, state.inner(), &session_id),
        || {
            job.paused.store(true, Ordering::SeqCst);
        },
    )
}

#[tauri::command]
pub fn resume_rewrite(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
) -> Result<DocumentSession, String> {
    let job = require_job(state.inner(), &session_id, "当前没有可继续的任务。")?;

    finish_rewrite_signal_steps(
        || rewrite_job_state::mark_session_running(&app, state.inner(), &session_id),
        || {
            job.paused.store(false, Ordering::SeqCst);
        },
    )
}

#[tauri::command]
pub fn cancel_rewrite(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
) -> Result<DocumentSession, String> {
    let maybe_job = load_job(state.inner(), &session_id)?;

    finish_rewrite_signal_steps(
        || rewrite_job_state::mark_session_cancelled(&app, state.inner(), &session_id),
        || {
            if let Some(job) = maybe_job {
                job.cancelled.store(true, Ordering::SeqCst);
            }
        },
    )
}

#[tauri::command]
pub async fn retry_rewrite_unit(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: String,
    rewrite_unit_id: String,
) -> Result<DocumentSession, String> {
    rewrite_jobs::process_rewrite_unit(&app, state.inner(), &session_id, &rewrite_unit_id, false)
        .await?;
    access_current_session(
        CurrentSessionRequest::stored(&app, state.inner(), &session_id),
        Ok,
    )
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    #[test]
    fn finish_rewrite_signal_steps_runs_mark_before_signal() {
        let calls = std::cell::RefCell::new(Vec::new());

        let result = super::finish_rewrite_signal_steps(
            || {
                calls.borrow_mut().push("mark".to_string());
                Ok::<_, String>("updated".to_string())
            },
            || {
                calls.borrow_mut().push("signal".to_string());
            },
        )
        .expect("expected rewrite signal helper to mark before signaling");

        assert_eq!(result, "updated");
        assert_eq!(
            calls.into_inner(),
            vec!["mark".to_string(), "signal".to_string()]
        );
    }

    #[test]
    fn finish_rewrite_signal_steps_skips_signal_when_mark_fails() {
        let signal_calls = Cell::new(0);

        let error = super::finish_rewrite_signal_steps(
            || Err::<(), String>("mark failed".to_string()),
            || {
                signal_calls.set(signal_calls.get() + 1);
            },
        )
        .expect_err("expected mark failure to short-circuit signal");

        assert_eq!(error, "mark failed");
        assert_eq!(signal_calls.get(), 0);
    }
}
