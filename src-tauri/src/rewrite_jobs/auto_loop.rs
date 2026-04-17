use std::{collections::HashSet, sync::Arc};

use tauri::{AppHandle, Manager};

use crate::{
    rewrite,
    rewrite_job_state::{clear_running_units, mark_units_running},
    session_access::{access_current_session, mutate_current_session, CurrentSessionRequest},
    session_edit::SessionMutation,
    state::{AppState, JobControl},
    storage,
};

use super::auto_runtime::AutoLoopRuntime;
use super::auto_state::{commit_auto_batch, ensure_in_flight_batches_drained};
use super::support::RewriteSessionAccess;
use super::{
    collect_rewrite_batch_source_texts, prepare_auto_rewrite_session, rewrite_session_request,
};

const AUTO_LOOP_UNINITIALIZED_CONCURRENCY: usize = 0;

trait BatchSettlement {
    fn remove_batch_checked(&mut self, rewrite_unit_ids: &[String]) -> Result<(), String>;
    fn apply_batch_result<T>(
        &mut self,
        rewrite_unit_ids: &[String],
        result: Result<T, String>,
    ) -> Result<T, String>;
    fn record_completed_checked(&mut self, completed_count: usize) -> Result<(), String>;
}

impl BatchSettlement for AutoLoopRuntime<'_> {
    fn remove_batch_checked(&mut self, rewrite_unit_ids: &[String]) -> Result<(), String> {
        let remove_result = self.remove_batch(rewrite_unit_ids);
        self.session_result(remove_result)
    }

    fn apply_batch_result<T>(
        &mut self,
        rewrite_unit_ids: &[String],
        result: Result<T, String>,
    ) -> Result<T, String> {
        self.batch_result(rewrite_unit_ids, result)
    }

    fn record_completed_checked(&mut self, completed_count: usize) -> Result<(), String> {
        let progress_result = self.record_completed_progress(completed_count);
        self.session_result(progress_result)
    }
}

pub(super) async fn run_auto_loop(
    app: AppHandle,
    session_id: String,
    job: Arc<JobControl>,
    target_unit_ids: Option<HashSet<String>>,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    let app_state = state.inner();
    let mut runtime = AutoLoopRuntime::new(
        &app,
        app_state,
        &session_id,
        &job,
        AUTO_LOOP_UNINITIALIZED_CONCURRENCY,
    );
    let settings = runtime.session_result(storage::load_settings(&app))?;
    runtime.apply_settings(&settings);
    let ensure_rewriteable_session = || {
        access_current_session(
            rewrite_session_request(&app, app_state, &session_id, RewriteSessionAccess::ActiveJob),
            Ok,
        )
    };
    runtime.session_result(ensure_rewriteable_session())?;

    let units_per_batch = settings.units_per_batch;
    let client = Arc::new(runtime.session_result(rewrite::build_client(&settings))?);

    let (total_units, mut pending, request_snapshot, completed_units) = runtime
        .session_result(mutate_current_session(
            CurrentSessionRequest::stored(&app, app_state, &session_id),
            |session| {
                let now = chrono::Utc::now();
                let touched = clear_running_units(session);
                let prepared = prepare_auto_rewrite_session(session, target_unit_ids.as_ref())?;
                let result = (
                    prepared.total_units,
                    prepared.pending,
                    prepared.request_snapshot,
                    prepared.completed_units,
                );

                if touched {
                    return Ok(SessionMutation::save(session, now, result));
                }

                Ok(SessionMutation::SkipSave(result))
            },
        ))?;

    runtime.set_progress_baseline(total_units, completed_units);
    runtime.session_result(runtime.emit_progress())?;

    if pending.is_empty() {
        return runtime.finish_successfully();
    }

    loop {
        if runtime.is_cancelled() {
            return runtime.cancel();
        }

        while !runtime.is_paused() && runtime.has_capacity() {
            let batch_unit_ids =
                crate::rewrite_targets::take_next_auto_batch(&mut pending, units_per_batch);
            if batch_unit_ids.is_empty() {
                break;
            }

            runtime.session_result(ensure_rewriteable_session())?;
            let batch_requests = runtime.session_result(collect_rewrite_batch_source_texts(
                &request_snapshot,
                &batch_unit_ids,
            ))?;
            let batch_request = runtime
                .session_result(super::support::build_rewrite_batch_request(batch_requests))?;
            runtime.session_result(mark_units_running(
                &app,
                app_state,
                &session_id,
                &batch_unit_ids,
            ))?;
            let batch_client = client.clone();
            let batch_settings = settings.clone();
            let start_batch_result = runtime.start_batch(batch_unit_ids.clone(), |tasks| {
                tasks.spawn(async move {
                    let result =
                        rewrite::rewrite_batch_with_client(&batch_client, &batch_settings, &batch_request)
                            .await;
                    (batch_unit_ids, result)
                });
            });
            runtime.session_result(start_batch_result)?;
        }

        if pending.is_empty() && !runtime.has_in_flight_batches() {
            break;
        }

        if !runtime.has_in_flight_batches() {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            continue;
        }

        match tokio::time::timeout(
            std::time::Duration::from_millis(250),
            runtime.tasks_mut().join_next(),
        )
        .await
        {
            Ok(Some(joined)) => match joined {
                Ok((rewrite_unit_ids, Ok(responses))) => {
                    finish_completed_batch_steps(
                        &mut runtime,
                        &rewrite_unit_ids,
                        |runtime: &mut AutoLoopRuntime<'_>| {
                            let completed_batch = match commit_auto_batch(
                                &app,
                                app_state,
                                &session_id,
                                &rewrite_unit_ids,
                                responses,
                            ) {
                                Ok(completed_batch) => completed_batch,
                                Err(error) => return runtime.settled_batch_error(error),
                            };
                            Ok(completed_batch.len())
                        },
                    )?;
                }
                Ok((rewrite_unit_ids, Err(error))) => {
                    return finish_failed_batch_steps(&mut runtime, &rewrite_unit_ids, error);
                }
                Err(join_error) => {
                    return runtime.session_result(Err(format!("后台任务异常退出：{join_error}")));
                }
            },
            Ok(None) => {
                let error = ensure_in_flight_batches_drained(runtime.in_flight_batches())
                    .expect_err(
                        "join set drained branch should only occur when in-flight batches remain",
                    );
                return runtime.session_result(Err(error));
            }
            Err(_) => {}
        }
    }

    runtime.finish_successfully()
}

fn finish_completed_batch_steps<S, Commit>(
    settlement: &mut S,
    rewrite_unit_ids: &[String],
    commit: Commit,
) -> Result<(), String>
where
    S: BatchSettlement,
    Commit: FnOnce(&mut S) -> Result<usize, String>,
{
    settlement.remove_batch_checked(rewrite_unit_ids)?;
    let completed_count = commit(settlement)?;
    settlement.record_completed_checked(completed_count)
}

fn finish_failed_batch_steps<S, T>(
    settlement: &mut S,
    rewrite_unit_ids: &[String],
    error: String,
) -> Result<T, String>
where
    S: BatchSettlement,
{
    settlement.remove_batch_checked(rewrite_unit_ids)?;
    settlement.apply_batch_result(rewrite_unit_ids, Err(error))
}

#[cfg(test)]
#[path = "auto_loop_tests.rs"]
mod tests;
