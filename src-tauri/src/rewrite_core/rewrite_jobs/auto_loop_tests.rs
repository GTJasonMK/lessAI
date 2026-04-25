use std::time::Duration;

use crate::rewrite_jobs::auto_state::{
    abort_in_flight, ensure_in_flight_batches_drained, remove_in_flight_batch,
    TASK_SET_DRAINED_WITH_IN_FLIGHT_BATCHES_ERROR, UNKNOWN_IN_FLIGHT_BATCH_ERROR,
};

#[derive(Default)]
struct TestBatchSettlement {
    calls: Vec<String>,
    remove_error: Option<String>,
    batch_error: Option<String>,
    progress_error: Option<String>,
}

impl super::BatchSettlement for TestBatchSettlement {
    fn remove_batch_checked(&mut self, rewrite_unit_ids: &[String]) -> Result<(), String> {
        self.calls.push(format!("remove:{rewrite_unit_ids:?}"));
        match self.remove_error.take() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    fn apply_batch_result<T>(
        &mut self,
        rewrite_unit_ids: &[String],
        result: Result<T, String>,
    ) -> Result<T, String> {
        self.calls.push(format!("batch:{rewrite_unit_ids:?}"));
        match self.batch_error.take() {
            Some(error) => Err(error),
            None => result,
        }
    }

    fn record_completed_checked(&mut self, completed_count: usize) -> Result<(), String> {
        self.calls.push(format!("progress:{completed_count}"));
        match self.progress_error.take() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

#[test]
fn abort_in_flight_clears_batches_and_aborts_tasks() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("build tokio runtime");

    runtime.block_on(async {
        let mut tasks = tokio::task::JoinSet::new();
        tasks.spawn(async {
            tokio::time::sleep(Duration::from_secs(60)).await;
            (
                vec!["unit-0".to_string()],
                Ok(crate::rewrite_unit::RewriteBatchResponse {
                    batch_id: "batch-1".to_string(),
                    results: Vec::new(),
                }),
            )
        });

        let mut batches = vec![vec!["unit-0".to_string()]];
        abort_in_flight(&mut tasks, &mut batches);

        assert!(
            batches.is_empty(),
            "expected abort to clear in-flight batches"
        );

        let joined = tasks
            .join_next()
            .await
            .expect("expected aborted task to remain joinable");
        let error = joined.expect_err("expected task to be aborted");
        assert!(
            error.is_cancelled(),
            "expected join error to report cancellation"
        );
    });
}

#[test]
fn remove_in_flight_batch_removes_matching_batch() {
    let mut batches = vec![
        vec!["unit-0".to_string(), "unit-1".to_string()],
        vec!["unit-2".to_string()],
    ];

    remove_in_flight_batch(&mut batches, &["unit-0".to_string(), "unit-1".to_string()])
        .expect("expected registered batch to be removed");

    assert_eq!(batches, vec![vec!["unit-2".to_string()]]);
}

#[test]
fn remove_in_flight_batch_rejects_unknown_batch() {
    let mut batches = vec![
        vec!["unit-0".to_string(), "unit-1".to_string()],
        vec!["unit-2".to_string()],
    ];

    let error = remove_in_flight_batch(&mut batches, &["unit-3".to_string()])
        .expect_err("expected unknown in-flight batch to be rejected");

    assert_eq!(error, UNKNOWN_IN_FLIGHT_BATCH_ERROR);
    assert_eq!(
        batches,
        vec![
            vec!["unit-0".to_string(), "unit-1".to_string()],
            vec!["unit-2".to_string()],
        ]
    );
}

#[test]
fn ensure_in_flight_batches_drained_rejects_remaining_batches() {
    let error = ensure_in_flight_batches_drained(&[
        vec!["unit-0".to_string()],
        vec!["unit-2".to_string(), "unit-3".to_string()],
    ])
    .expect_err("expected orphaned in-flight batches to be rejected");

    assert_eq!(error, TASK_SET_DRAINED_WITH_IN_FLIGHT_BATCHES_ERROR);
}

#[test]
fn ensure_in_flight_batches_drained_allows_empty_state() {
    ensure_in_flight_batches_drained(&[]).expect("expected empty in-flight state to be accepted");
}

#[test]
fn finish_completed_batch_steps_runs_remove_commit_then_progress() {
    let mut settlement = TestBatchSettlement::default();
    let batch = vec!["unit-1".to_string(), "unit-2".to_string()];

    super::finish_completed_batch_steps(&mut settlement, &batch, |settlement| {
        settlement.calls.push("commit".to_string());
        super::BatchSettlement::apply_batch_result(settlement, &batch, Ok(2usize))
    })
    .expect("expected completed batch helper to run remove, commit, then progress");

    assert_eq!(
        settlement.calls,
        vec![
            "remove:[\"unit-1\", \"unit-2\"]".to_string(),
            "commit".to_string(),
            "batch:[\"unit-1\", \"unit-2\"]".to_string(),
            "progress:2".to_string(),
        ]
    );
}

#[test]
fn finish_completed_batch_steps_stops_before_progress_when_commit_fails() {
    let mut settlement = TestBatchSettlement::default();
    let batch = vec!["unit-3".to_string()];

    let error = super::finish_completed_batch_steps(&mut settlement, &batch, |_| {
        Err::<usize, String>("commit failed".to_string())
    })
    .expect_err("expected commit failure to short-circuit progress");

    assert_eq!(error, "commit failed");
    assert_eq!(settlement.calls, vec!["remove:[\"unit-3\"]".to_string()]);
}

#[test]
fn finish_failed_batch_steps_runs_remove_before_failure_handler() {
    let mut settlement = TestBatchSettlement::default();
    let batch = vec!["unit-4".to_string()];

    let error = super::finish_failed_batch_steps::<_, ()>(
        &mut settlement,
        &batch,
        "batch failed".to_string(),
    )
    .expect_err("expected failed batch helper to surface batch failure");

    assert_eq!(error, "batch failed");
    assert_eq!(
        settlement.calls,
        vec![
            "remove:[\"unit-4\"]".to_string(),
            "batch:[\"unit-4\"]".to_string(),
        ]
    );
}
