use std::path::Path;
use std::collections::HashSet;

use log::{error, info};

use crate::{
    documents::{
        ensure_document_can_ai_rewrite, ensure_document_can_write_back, execute_document_writeback,
        is_docx_path, OwnedDocumentWriteback, WritebackMode,
    },
    models::{DocumentSession, SuggestionDecision},
    observability::{document_kind_label, writeback_mode_label},
    rewrite_permissions::ensure_rewrite_unit_can_rewrite,
    rewrite_projection::{apply_preview_suggestion, build_applied_slot_projection},
    rewrite_unit::{merged_text_from_slots, RewriteUnitResponse},
};

type SessionWritebackPlan = OwnedDocumentWriteback;

pub(crate) fn validate_candidate_batch_writeback(
    session: &DocumentSession,
    responses: &[RewriteUnitResponse],
) -> Result<(), String> {
    validate_unique_batch_slot_updates(responses)?;
    let preview = build_preview_session(session, responses)?;
    execute_session_writeback(&preview, WritebackMode::Validate)
}

pub(crate) fn execute_session_writeback(
    session: &DocumentSession,
    mode: WritebackMode,
) -> Result<(), String> {
    let path = Path::new(&session.document_path);
    info!(
        "session writeback started: session_id={} mode={} document_kind={} path={}",
        session.id,
        writeback_mode_label(mode),
        document_kind_label(&session.document_path),
        session.document_path,
    );

    let result = (|| {
        if mode == WritebackMode::Write {
            ensure_document_can_write_back(&session.document_path)?;
        }
        ensure_applied_suggestions_target_rewriteable(session)?;
        ensure_document_can_ai_rewrite(
            path,
            session.write_back_supported,
            session.write_back_block_reason.as_deref(),
        )?;

        let plan = build_session_writeback_plan(session)?;
        execute_document_writeback(
            path,
            &session.source_text,
            session.source_snapshot.as_ref(),
            plan.as_document_writeback(),
            mode,
        )
    })();

    match &result {
        Ok(()) => info!(
            "session writeback finished: session_id={} mode={} document_kind={} path={}",
            session.id,
            writeback_mode_label(mode),
            document_kind_label(&session.document_path),
            session.document_path,
        ),
        Err(message) => error!(
            "session writeback failed: session_id={} mode={} document_kind={} path={} error={message}",
            session.id,
            writeback_mode_label(mode),
            document_kind_label(&session.document_path),
            session.document_path,
        ),
    }

    result
}

fn build_preview_session(
    session: &DocumentSession,
    responses: &[RewriteUnitResponse],
) -> Result<DocumentSession, String> {
    let mut preview = session.clone();
    for response in responses {
        ensure_rewrite_unit_can_rewrite(&preview, &response.rewrite_unit_id)?;
        apply_preview_suggestion(
            &mut preview,
            &response.rewrite_unit_id,
            response.updates.clone(),
        )?;
    }
    Ok(preview)
}

fn validate_unique_batch_slot_updates(responses: &[RewriteUnitResponse]) -> Result<(), String> {
    let mut seen = HashSet::new();
    for response in responses {
        for update in &response.updates {
            if seen.insert(update.slot_id.as_str()) {
                continue;
            }
            return Err(format!(
                "写回内容与原结构不一致：batch 内存在重复 slot 更新：{}。",
                update.slot_id
            ));
        }
    }
    Ok(())
}

fn build_session_writeback_plan(session: &DocumentSession) -> Result<SessionWritebackPlan, String> {
    let updated_slots = build_applied_slot_projection(session)?;
    if !is_docx_path(Path::new(&session.document_path)) {
        return Ok(SessionWritebackPlan::Text(merged_text_from_slots(&updated_slots)));
    }

    Ok(SessionWritebackPlan::Slots(updated_slots))
}

fn ensure_applied_suggestions_target_rewriteable(session: &DocumentSession) -> Result<(), String> {
    for suggestion in session
        .suggestions
        .iter()
        .filter(|item| item.decision == SuggestionDecision::Applied)
    {
        ensure_rewrite_unit_can_rewrite(session, &suggestion.rewrite_unit_id)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "rewrite_writeback_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "rewrite_writeback_fixture_tests.rs"]
mod fixture_tests;
