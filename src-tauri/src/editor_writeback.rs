use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use crate::{
    documents::{
        ensure_document_can_write_back, execute_document_writeback, is_docx_path,
        normalize_text_against_source_layout, OwnedDocumentWriteback, WritebackMode,
    },
    models::{RewriteUnitStatus, DocumentSession, EditorSlotEdit, RunningState},
    rewrite_unit::{apply_slot_updates, SlotUpdate},
};

const EDITOR_WRITEBACK_CONFLICT_ERROR: &str =
    "该文档存在修订记录或进度，为避免冲突，请先“覆写并清理记录”或“重置记录”后再编辑。";

pub(crate) type EditorWritebackPayload = OwnedDocumentWriteback;

pub(crate) fn ensure_session_can_use_plain_text_editor(
    session: &DocumentSession,
) -> Result<(), String> {
    ensure_document_can_write_back(&session.document_path)?;
    if !session.plain_text_editor_safe {
        return Err(session
            .plain_text_editor_block_reason
            .clone()
            .unwrap_or_else(|| "当前文档暂不支持进入编辑模式。".to_string()));
    }
    if !plain_text_editor_session_is_clean(session) {
        return Err(EDITOR_WRITEBACK_CONFLICT_ERROR.to_string());
    }
    Ok(())
}

pub(crate) fn build_plain_text_editor_writeback(
    session: &DocumentSession,
    content: &str,
) -> Result<EditorWritebackPayload, String> {
    if content.trim().is_empty() {
        return Err("文档内容为空，无法保存。".to_string());
    }
    ensure_session_can_use_plain_text_editor(session)?;
    if is_docx_path(Path::new(&session.document_path)) {
        return Err("docx 编辑模式必须按槽位保存，不能再走整篇纯文本写回。".to_string());
    }

    Ok(EditorWritebackPayload::Text(
        normalize_text_against_source_layout(&session.source_text, content),
    ))
}

pub(crate) fn build_slot_editor_writeback(
    session: &DocumentSession,
    edits: &[EditorSlotEdit],
) -> Result<EditorWritebackPayload, String> {
    ensure_session_can_use_plain_text_editor(session)?;
    if !is_docx_path(Path::new(&session.document_path)) {
        return Err("当前仅 docx 支持按槽位编辑写回。".to_string());
    }
    let slot_updates = collect_slot_edit_updates(session, edits)?;
    let updated_slots = apply_slot_updates(&session.writeback_slots, &slot_updates)?;
    Ok(EditorWritebackPayload::Slots(updated_slots))
}

pub(crate) fn execute_editor_writeback(
    session: &DocumentSession,
    payload: &EditorWritebackPayload,
    mode: WritebackMode,
) -> Result<(), String> {
    execute_document_writeback(
        Path::new(&session.document_path),
        &session.source_text,
        session.source_snapshot.as_ref(),
        payload.as_document_writeback(),
        mode,
    )
}

fn plain_text_editor_session_is_clean(session: &DocumentSession) -> bool {
    session.status == RunningState::Idle
        && session.suggestions.is_empty()
        && session
            .rewrite_units
            .iter()
            .all(|unit| unit.status == RewriteUnitStatus::Idle || unit.status == RewriteUnitStatus::Done)
}

fn collect_slot_edit_updates(
    session: &DocumentSession,
    edits: &[EditorSlotEdit],
) -> Result<Vec<SlotUpdate>, String> {
    let editable_slot_ids = session
        .writeback_slots
        .iter()
        .filter(|slot| slot.editable)
        .map(|slot| slot.id.clone())
        .collect::<HashSet<_>>();
    if edits.len() != editable_slot_ids.len() {
        return Err("编辑器提交的可编辑槽位数量与当前会话不一致，请重新进入编辑模式。".to_string());
    }

    let mut seen = HashMap::with_capacity(edits.len());
    for edit in edits {
        if !editable_slot_ids.contains(&edit.slot_id) {
            return Err(format!(
                "编辑器提交了不可编辑或不存在的槽位 {}, 无法安全写回。",
                edit.slot_id
            ));
        }
        if seen.insert(edit.slot_id.clone(), edit.text.clone()).is_some() {
            return Err(format!(
                "编辑器提交了重复的槽位 {}, 无法安全写回。",
                edit.slot_id
            ));
        }
    }

    Ok(edits
        .iter()
        .map(|edit| SlotUpdate::new(&edit.slot_id, &edit.text))
        .collect())
}

#[cfg(test)]
#[path = "editor_writeback_tests.rs"]
mod tests;
