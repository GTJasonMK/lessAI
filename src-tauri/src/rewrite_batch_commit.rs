use chrono::Utc;
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

use crate::{
    documents::normalize_text_against_source_layout,
    models::{RewriteUnitStatus, RewriteUnitCompletedEvent, DocumentSession, RunningState, SuggestionDecision},
    rewrite,
    rewrite_job_state::fail_target_units_and_reset_other_running,
    rewrite_projection::apply_suggestion_by_id,
    rewrite_unit::{
        rewrite_unit_text, rewrite_unit_text_with_updates, RewriteBatchResponse,
        RewriteSuggestion, RewriteUnitResponse,
    },
    session_access::{mutate_current_session, CurrentSessionRequest},
    session_edit::SessionMutation,
    state::AppState,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BatchCommitMode {
    pub(crate) decision: SuggestionDecision,
    pub(crate) set_status: Option<RunningState>,
}

pub(crate) fn batch_commit_mode(auto_approve: bool) -> BatchCommitMode {
    if auto_approve {
        return BatchCommitMode {
            decision: SuggestionDecision::Applied,
            set_status: None,
        };
    }
    BatchCommitMode {
        decision: SuggestionDecision::Proposed,
        set_status: Some(RunningState::Idle),
    }
}

pub(crate) fn rewrite_unit_completed_events(
    session_id: &str,
    completed_batch: &[(String, String, u64)],
) -> Vec<RewriteUnitCompletedEvent> {
    completed_batch
        .iter()
        .map(
            |(rewrite_unit_id, suggestion_id, suggestion_sequence)| RewriteUnitCompletedEvent {
                session_id: session_id.to_string(),
                rewrite_unit_id: rewrite_unit_id.clone(),
                suggestion_id: suggestion_id.clone(),
                suggestion_sequence: *suggestion_sequence,
            },
        )
        .collect()
}

pub(crate) fn emit_rewrite_unit_completed_events(
    app: &AppHandle,
    session_id: &str,
    completed_batch: &[(String, String, u64)],
) -> Result<(), String> {
    for event in rewrite_unit_completed_events(session_id, completed_batch) {
        app.emit("rewrite_unit_completed", event)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub(crate) fn commit_rewrite_result(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
    rewrite_unit_ids: &[String],
    rewrite_result: Result<RewriteBatchResponse, String>,
    mode: BatchCommitMode,
    validate_batch_writeback: impl FnOnce(
        &DocumentSession,
        &[RewriteUnitResponse],
    ) -> Result<(), String>,
) -> Result<Vec<(String, String, u64)>, String> {
    let commit_result = match rewrite_result {
        Ok(response) => commit_rewrite_batch_success(
            app,
            state,
            session_id,
            rewrite_unit_ids,
            response,
            mode,
            validate_batch_writeback,
        ),
        Err(error) => Err(error),
    };

    match commit_result {
        Ok(completed) => Ok(completed),
        Err(error) => {
            commit_units_failure(app, state, session_id, rewrite_unit_ids, error.clone())?;
            Err(error)
        }
    }
}

fn commit_rewrite_batch_success(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
    rewrite_unit_ids: &[String],
    response: RewriteBatchResponse,
    mode: BatchCommitMode,
    validate_batch_writeback: impl FnOnce(
        &DocumentSession,
        &[RewriteUnitResponse],
    ) -> Result<(), String>,
) -> Result<Vec<(String, String, u64)>, String> {
    mutate_current_session(
        CurrentSessionRequest::stored(app, state, session_id),
        |latest| {
            let now = Utc::now();
            let normalized = normalize_candidate_batch_response(latest, rewrite_unit_ids, response)?;
            validate_batch_writeback(latest, &normalized)?;
            let mut completed = Vec::with_capacity(rewrite_unit_ids.len());

            for response in &normalized {
                let suggestion = create_committed_suggestion(latest, response, now)?;
                apply_committed_suggestion(latest, suggestion.clone(), mode.decision)?;
                completed.push((
                    response.rewrite_unit_id.clone(),
                    suggestion.id,
                    suggestion.sequence,
                ));
            }

            if let Some(status) = mode.set_status {
                latest.status = status;
            }

            Ok(SessionMutation::save(latest, now, completed))
        },
    )
}

fn normalize_candidate_batch_response(
    session: &DocumentSession,
    rewrite_unit_ids: &[String],
    response: RewriteBatchResponse,
) -> Result<Vec<RewriteUnitResponse>, String> {
    let responses = response.results;
    if rewrite_unit_ids.len() != responses.len() {
        return Err("批量改写结果数量与目标改写单元数量不一致。".to_string());
    }

    let mut normalized = Vec::with_capacity(responses.len());
    for (rewrite_unit_id, response) in rewrite_unit_ids.iter().zip(responses.into_iter()) {
        if &response.rewrite_unit_id != rewrite_unit_id {
            return Err("批量改写结果与目标改写单元顺序不一致。".to_string());
        }
        let mut updates = Vec::with_capacity(response.updates.len());
        for update in response.updates {
            let source_slot = session
                .writeback_slots
                .iter()
                .find(|slot| slot.id == update.slot_id)
                .ok_or_else(|| format!("未知 slot_id：{}。", update.slot_id))?;
            updates.push(crate::rewrite_unit::SlotUpdate::new(
                &update.slot_id,
                &normalize_text_against_source_layout(&source_slot.text, &update.text),
            ));
        }
        normalized.push(RewriteUnitResponse {
            rewrite_unit_id: response.rewrite_unit_id,
            updates,
        });
    }
    Ok(normalized)
}

fn create_committed_suggestion(
    session: &mut DocumentSession,
    response: &RewriteUnitResponse,
    now: chrono::DateTime<Utc>,
) -> Result<RewriteSuggestion, String> {
    let before_text = rewrite_unit_text(session, &response.rewrite_unit_id)?;
    let after_text =
        rewrite_unit_text_with_updates(session, &response.rewrite_unit_id, &response.updates)?;
    let suggestion = RewriteSuggestion {
        id: Uuid::new_v4().to_string(),
        sequence: session.next_suggestion_sequence,
        rewrite_unit_id: response.rewrite_unit_id.clone(),
        before_text: before_text.clone(),
        after_text: after_text.clone(),
        diff_spans: rewrite::build_diff(&before_text, &after_text),
        decision: SuggestionDecision::Applied,
        slot_updates: response.updates.clone(),
        created_at: now,
        updated_at: now,
    };
    session.next_suggestion_sequence = session.next_suggestion_sequence.saturating_add(1);
    Ok(suggestion)
}

fn apply_committed_suggestion(
    session: &mut DocumentSession,
    suggestion: RewriteSuggestion,
    decision: SuggestionDecision,
) -> Result<(), String> {
    let now = suggestion.updated_at;
    let suggestion_id = suggestion.id.clone();
    let rewrite_unit_id = suggestion.rewrite_unit_id.clone();
    let is_applied = decision == SuggestionDecision::Applied;
    let mut suggestion = suggestion;

    suggestion.decision = decision;
    session.suggestions.push(suggestion);
    if is_applied {
        apply_suggestion_by_id(session, &suggestion_id, now)?;
    }

    let unit = session
        .rewrite_units
        .iter_mut()
        .find(|unit| unit.id == rewrite_unit_id)
        .ok_or_else(|| "未找到对应的改写单元。".to_string())?;
    unit.status = RewriteUnitStatus::Done;
    unit.error_message = None;
    Ok(())
}

fn commit_units_failure(
    app: &AppHandle,
    state: &AppState,
    session_id: &str,
    rewrite_unit_ids: &[String],
    error: String,
) -> Result<(), String> {
    mutate_current_session(
        CurrentSessionRequest::stored(app, state, session_id),
        |session| {
            let now = Utc::now();
            fail_target_units_and_reset_other_running(session, rewrite_unit_ids, &error)?;
            Ok(SessionMutation::save(session, now, ()))
        },
    )
}

#[cfg(test)]
#[path = "rewrite_batch_commit_tests.rs"]
mod tests;
