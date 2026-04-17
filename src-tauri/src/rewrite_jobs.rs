mod auto;
mod auto_loop;
mod auto_runtime;
mod auto_state;
mod manual;
mod process;
mod support;

pub(crate) use auto::run_auto_rewrite;
pub(crate) use manual::run_manual_rewrite;
pub(crate) use process::process_rewrite_unit;
use process::process_loaded_rewrite_batch;
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
