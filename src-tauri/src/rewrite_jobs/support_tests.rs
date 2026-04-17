#[test]
fn rewrite_session_access_scope_only_blocks_external_entries() {
    assert_eq!(
        super::rewrite_session_active_job_error(super::RewriteSessionAccess::ExternalEntry),
        Some(super::ACTIVE_REWRITE_SESSION_ERROR)
    );
    assert_eq!(
        super::rewrite_session_active_job_error(super::RewriteSessionAccess::ActiveJob),
        None
    );
}

#[test]
fn in_flight_batch_count_uses_batch_count_not_unit_count() {
    let batches = vec![
        vec!["unit-0".to_string(), "unit-1".to_string()],
        vec!["unit-2".to_string()],
    ];

    assert_eq!(super::in_flight_batch_count(&batches), 2);
    assert_eq!(super::snapshot_running_indices_from_batches(&batches).len(), 3);
}
