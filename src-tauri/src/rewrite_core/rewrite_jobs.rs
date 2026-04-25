#[path = "rewrite_jobs/auto.rs"]
mod auto;
#[path = "rewrite_jobs/auto_loop.rs"]
mod auto_loop;
#[path = "rewrite_jobs/auto_runtime.rs"]
mod auto_runtime;
#[path = "rewrite_jobs/auto_state.rs"]
mod auto_state;
#[path = "rewrite_jobs/manual.rs"]
mod manual;
#[path = "rewrite_jobs/process.rs"]
mod process;
#[path = "rewrite_jobs/support.rs"]
mod support;

pub(crate) use auto::run_auto_rewrite;
pub(crate) use manual::run_manual_rewrite;
use process::process_loaded_rewrite_batch;
pub(crate) use process::process_rewrite_unit;
#[cfg(test)]
use support::build_rewrite_source_snapshot;
use support::{
    auto_running_state, collect_rewrite_batch_source_texts, emit_rewrite_finished,
    emit_rewrite_progress, ensure_targets_available, in_flight_batch_count,
    prepare_auto_rewrite_session, prepare_loaded_rewrite_batch, resolve_available_rewrite_targets,
    rewrite_session_request, snapshot_running_indices_from_batches,
};

#[cfg(test)]
#[path = "rewrite_jobs_tests.rs"]
mod tests;
