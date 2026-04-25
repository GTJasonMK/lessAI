use crate::{
    models::RewriteUnitStatus,
    rewrite_unit::{RewriteBatchResponse, RewriteUnitResponse, SlotUpdate},
    test_support::{editable_slot, rewrite_unit, sample_clean_session},
};

fn sample_session() -> crate::models::DocumentSession {
    let mut session = sample_clean_session("session-1", "/tmp/example.txt", "原文");
    session.writeback_slots = vec![editable_slot("slot-0", 0, "原文")];
    session.rewrite_units = vec![rewrite_unit(
        "unit-0",
        0,
        &["slot-0"],
        "原文",
        RewriteUnitStatus::Idle,
    )];
    session
}

#[test]
fn normalize_candidate_batch_response_reads_batch_results() {
    let session = sample_session();

    let normalized = super::normalize_candidate_batch_response(
        &session,
        &["unit-0".to_string()],
        RewriteBatchResponse {
            batch_id: "batch-1".to_string(),
            results: vec![RewriteUnitResponse {
                rewrite_unit_id: "unit-0".to_string(),
                updates: vec![SlotUpdate::new("slot-0", "改写后")],
            }],
        },
    )
    .expect("batch response should normalize");

    assert_eq!(normalized.len(), 1);
    assert_eq!(normalized[0].rewrite_unit_id, "unit-0");
    assert_eq!(normalized[0].updates[0].slot_id, "slot-0");
    assert_eq!(normalized[0].updates[0].text, "改写后");
}

#[test]
fn normalize_candidate_batch_response_rejects_batch_count_mismatch() {
    let session = sample_session();

    let error = super::normalize_candidate_batch_response(
        &session,
        &["unit-0".to_string()],
        RewriteBatchResponse {
            batch_id: "batch-1".to_string(),
            results: Vec::new(),
        },
    )
    .expect_err("count mismatch should be rejected");

    assert_eq!(error, "批量改写结果数量与目标改写单元数量不一致。");
}
