use chrono::{DateTime, Utc};

use crate::{
    models::{DocumentSession, SuggestionDecision},
    rewrite,
    rewrite_unit::{
        apply_slot_updates, rewrite_unit_text, rewrite_unit_text_with_updates, SlotUpdate,
    },
};

pub(crate) const SUGGESTION_NOT_FOUND_ERROR: &str = "未找到对应的修改对。";

pub(crate) fn dismiss_applied_suggestions_for_unit(
    session: &mut DocumentSession,
    rewrite_unit_id: &str,
    now: DateTime<Utc>,
) {
    for suggestion in &mut session.suggestions {
        if suggestion.rewrite_unit_id == rewrite_unit_id
            && suggestion.decision == SuggestionDecision::Applied
        {
            suggestion.decision = SuggestionDecision::Dismissed;
            suggestion.updated_at = now;
        }
    }
}

pub(crate) fn apply_preview_suggestion(
    session: &mut DocumentSession,
    rewrite_unit_id: &str,
    slot_updates: Vec<SlotUpdate>,
) -> Result<(), String> {
    let before_text = rewrite_unit_text(session, rewrite_unit_id)?;
    let after_text = rewrite_unit_text_with_updates(session, rewrite_unit_id, &slot_updates)?;
    let now = Utc::now();
    dismiss_applied_suggestions_for_unit(session, rewrite_unit_id, now);
    session
        .suggestions
        .push(crate::rewrite_unit::RewriteSuggestion {
            id: format!("__preview__:{rewrite_unit_id}"),
            sequence: session.next_suggestion_sequence,
            rewrite_unit_id: rewrite_unit_id.to_string(),
            before_text: before_text.clone(),
            after_text: after_text.clone(),
            diff: rewrite::build_diff_result(&before_text, &after_text),
            decision: SuggestionDecision::Applied,
            slot_updates,
            created_at: now,
            updated_at: now,
        });
    Ok(())
}

pub(crate) fn apply_suggestion_by_id(
    session: &mut DocumentSession,
    suggestion_id: &str,
    now: DateTime<Utc>,
) -> Result<String, String> {
    let suggestion_index = find_suggestion_index(session, suggestion_id)?;
    let rewrite_unit_id = session
        .suggestions
        .get(suggestion_index)
        .ok_or_else(|| SUGGESTION_NOT_FOUND_ERROR.to_string())?
        .rewrite_unit_id
        .clone();
    dismiss_applied_suggestions_for_unit(session, &rewrite_unit_id, now);
    let suggestion = session
        .suggestions
        .get_mut(suggestion_index)
        .ok_or_else(|| SUGGESTION_NOT_FOUND_ERROR.to_string())?;
    suggestion.decision = SuggestionDecision::Applied;
    suggestion.updated_at = now;
    Ok(rewrite_unit_id)
}

pub(crate) fn find_suggestion_index(
    session: &DocumentSession,
    suggestion_id: &str,
) -> Result<usize, String> {
    session
        .suggestions
        .iter()
        .position(|item| item.id == suggestion_id)
        .ok_or_else(|| SUGGESTION_NOT_FOUND_ERROR.to_string())
}

pub(crate) fn build_applied_slot_projection(
    session: &DocumentSession,
) -> Result<Vec<crate::rewrite_unit::WritebackSlot>, String> {
    let mut projected = session.writeback_slots.clone();
    let mut applied = session
        .suggestions
        .iter()
        .filter(|suggestion| suggestion.decision == SuggestionDecision::Applied)
        .collect::<Vec<_>>();
    applied.sort_by_key(|suggestion| suggestion.sequence);

    for suggestion in applied {
        projected = apply_slot_updates(&projected, &suggestion.slot_updates)?;
    }

    Ok(projected)
}
