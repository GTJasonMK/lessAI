use std::cell::Cell;

#[test]
fn run_with_auto_failure_steps_returns_ok_without_running_handler() {
    let handler_calls = Cell::new(0);

    let result = super::run_with_auto_failure(Ok::<_, String>("ok".to_string()), |_| {
        handler_calls.set(handler_calls.get() + 1);
        Ok(())
    })
    .expect("expected ok result to bypass failure handler");

    assert_eq!(result, "ok");
    assert_eq!(handler_calls.get(), 0);
}

#[test]
fn run_with_auto_failure_steps_delegates_error_to_failure_handler() {
    let handler_calls = Cell::new(0);

    let error =
        super::run_with_auto_failure(Err::<(), String>("loop failed".to_string()), |error| {
            handler_calls.set(handler_calls.get() + 1);
            Err(format!("handled:{error}"))
        })
        .expect_err("expected error result to go through failure handler");

    assert_eq!(error, "handled:loop failed");
    assert_eq!(handler_calls.get(), 1);
}
