use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use crate::{
    documents::{
        ensure_capability_allowed, execute_document_writeback, normalize_text_against_source_layout,
        DocumentWritebackContext, OwnedDocumentWriteback, WritebackMode,
    },
    models::{DocumentSession, EditorSlotEdit},
    rewrite_unit::{apply_slot_updates, SlotUpdate},
};
use crate::session_capability_models::DocumentEditorMode;

pub(crate) type EditorWritebackPayload = OwnedDocumentWriteback;

pub(crate) fn ensure_session_can_use_editor_writeback(
    session: &DocumentSession,
) -> Result<(), String> {
    ensure_capability_allowed(
        &session.capabilities.source_writeback,
        "当前文档暂不支持写回原文件。",
    )?;
    ensure_capability_allowed(
        &session.capabilities.editor_entry,
        "当前文档暂不支持进入编辑模式。",
    )
}

pub(crate) fn build_full_text_editor_writeback(
    session: &DocumentSession,
    content: &str,
) -> Result<EditorWritebackPayload, String> {
    if content.trim().is_empty() {
        return Err("文档内容为空，无法保存。".to_string());
    }
    ensure_session_can_use_editor_writeback(session)?;
    match session.capabilities.editor_mode {
        DocumentEditorMode::FullText => {}
        DocumentEditorMode::SlotBased => {
            return Err(
                "结构化编辑模式必须按槽位保存，不能再走整篇纯文本写回。".to_string(),
            )
        }
        DocumentEditorMode::None => {
            return Err("当前文档暂不支持整篇纯文本编辑写回。".to_string())
        }
    }

    Ok(EditorWritebackPayload::Text(
        normalize_text_against_source_layout(&session.source_text, content),
    ))
}

pub(crate) fn build_slot_editor_writeback(
    session: &DocumentSession,
    edits: &[EditorSlotEdit],
) -> Result<EditorWritebackPayload, String> {
    ensure_session_can_use_editor_writeback(session)?;
    if session.capabilities.editor_mode != DocumentEditorMode::SlotBased {
        return Err("当前仅槽位编辑文档支持按槽位写回。".to_string());
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
        DocumentWritebackContext::from_session(session),
        payload.as_document_writeback(),
        mode,
    )
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
        if seen
            .insert(edit.slot_id.clone(), edit.text.clone())
            .is_some()
        {
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
