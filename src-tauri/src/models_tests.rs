use super::{AppSettings, SegmentationPreset, DocumentSession, RunningState};
use crate::test_support::sample_clean_session;

#[test]
fn rejects_legacy_segmentation_preset_aliases() {
    for legacy in ["small", "medium", "large", "question"] {
        let payload = format!("\"{legacy}\"");
        let parsed = serde_json::from_str::<SegmentationPreset>(&payload);
        assert!(
            parsed.is_err(),
            "legacy preset should be rejected: {legacy}"
        );
    }
}

#[test]
fn accepts_current_segmentation_preset_values() {
    assert_eq!(
        serde_json::from_str::<SegmentationPreset>("\"clause\"").unwrap(),
        SegmentationPreset::Clause
    );
    assert_eq!(
        serde_json::from_str::<SegmentationPreset>("\"sentence\"").unwrap(),
        SegmentationPreset::Sentence
    );
    assert_eq!(
        serde_json::from_str::<SegmentationPreset>("\"paragraph\"").unwrap(),
        SegmentationPreset::Paragraph
    );
}

#[test]
fn rejects_app_settings_payload_missing_current_required_fields() {
    let base = serde_json::json!({
        "baseUrl": "https://api.openai.com/v1",
        "apiKey": "",
        "model": "gpt-4.1-mini",
        "updateProxy": "",
        "timeoutMs": 45000,
        "temperature": 0.8,
        "segmentationPreset": "paragraph",
        "rewriteHeadings": false,
        "rewriteMode": "manual",
        "maxConcurrency": 2,
        "unitsPerBatch": 1,
        "promptPresetId": "humanizer_zh",
        "customPrompts": []
    });

    for field in [
        "updateProxy",
        "rewriteHeadings",
        "maxConcurrency",
        "unitsPerBatch",
        "promptPresetId",
        "customPrompts",
    ] {
        let mut payload = base.clone();
        payload
            .as_object_mut()
            .expect("object")
            .remove(field)
            .expect("field exists");

        let parsed = serde_json::from_value::<AppSettings>(payload);
        assert!(parsed.is_err(), "missing field should be rejected: {field}");
    }
}

fn sample_session(status: RunningState) -> DocumentSession {
    let mut session = sample_clean_session("session-1", "/tmp/example.txt", "正文");
    session.status = status;
    session
}

#[test]
fn running_state_identifies_active_job_states() {
    assert!(RunningState::Running.is_active_job());
    assert!(RunningState::Paused.is_active_job());
    assert!(!RunningState::Idle.is_active_job());
    assert!(!RunningState::Completed.is_active_job());
    assert!(!RunningState::Cancelled.is_active_job());
    assert!(!RunningState::Failed.is_active_job());
}

#[test]
fn document_session_downgrades_only_active_job_states() {
    let mut active = sample_session(RunningState::Running);
    assert!(active.downgrade_active_job_to_cancelled());
    assert_eq!(active.status, RunningState::Cancelled);

    let mut idle = sample_session(RunningState::Idle);
    assert!(!idle.downgrade_active_job_to_cancelled());
    assert_eq!(idle.status, RunningState::Idle);
}
