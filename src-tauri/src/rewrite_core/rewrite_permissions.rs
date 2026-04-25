use crate::{
    documents::ensure_document_can_ai_rewrite_safely, models::DocumentSession,
    rewrite_unit::find_rewrite_unit,
};

pub(crate) const REWRITE_UNIT_NOT_FOUND_ERROR: &str = "改写单元不存在。";

pub(crate) fn protected_rewrite_unit_error(rewrite_unit_id: &str) -> String {
    format!("改写单元 {rewrite_unit_id} 属于保护区，不允许 AI 改写。")
}

pub(crate) fn ensure_session_can_rewrite(session: &DocumentSession) -> Result<(), String> {
    ensure_document_can_ai_rewrite_safely(
        std::path::Path::new(&session.document_path),
        session.source_snapshot.as_ref(),
        &session.capabilities.ai_rewrite,
    )
}

pub(crate) fn ensure_rewrite_unit_can_rewrite(
    session: &DocumentSession,
    rewrite_unit_id: &str,
) -> Result<(), String> {
    let unit = find_rewrite_unit(session, rewrite_unit_id)
        .map_err(|_| REWRITE_UNIT_NOT_FOUND_ERROR.to_string())?;
    if unit.status == crate::models::RewriteUnitStatus::Done
        && unit.slot_ids.iter().all(|slot_id| {
            session
                .writeback_slots
                .iter()
                .find(|slot| slot.id == *slot_id)
                .is_some_and(|slot| !slot.editable)
        })
    {
        return Err(protected_rewrite_unit_error(rewrite_unit_id));
    }
    Ok(())
}
