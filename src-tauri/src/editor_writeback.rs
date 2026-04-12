use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use crate::{
    documents::{ensure_document_can_write_back, is_docx_path},
    models::{ChunkStatus, DocumentSession, EditorChunkEdit, RunningState},
    rewrite,
};

const EDITOR_WRITEBACK_CONFLICT_ERROR: &str =
    "该文档存在修订记录或进度，为避免冲突，请先“覆写并清理记录”或“重置记录”后再编辑。";

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

pub(crate) fn normalize_editor_writeback_content(
    document_path: &str,
    source_text: &str,
    content: &str,
) -> String {
    if is_docx_path(Path::new(document_path)) {
        return content.to_string();
    }

    let mut processed = content.to_string();
    let line_ending = rewrite::detect_line_ending(source_text);
    if !rewrite::has_trailing_spaces_per_line(source_text) {
        processed = rewrite::strip_trailing_spaces_per_line(&processed);
    }
    rewrite::convert_line_endings(&processed, line_ending)
}

pub(crate) fn build_updated_text_from_chunk_edits(
    session: &DocumentSession,
    edits: &[EditorChunkEdit],
) -> Result<String, String> {
    ensure_session_can_use_plain_text_editor(session)?;
    let overrides = collect_chunk_edit_overrides(session, edits)?;
    Ok(session
        .chunks
        .iter()
        .map(|chunk| {
            let body = overrides
                .get(&chunk.index)
                .cloned()
                .unwrap_or_else(|| chunk.source_text.clone());
            format!("{body}{}", chunk.separator_after)
        })
        .collect::<String>())
}

fn plain_text_editor_session_is_clean(session: &DocumentSession) -> bool {
    session.status == RunningState::Idle
        && session.suggestions.is_empty()
        && session
            .chunks
            .iter()
            .all(|chunk| chunk.status == ChunkStatus::Idle || chunk.skip_rewrite)
}

fn collect_chunk_edit_overrides(
    session: &DocumentSession,
    edits: &[EditorChunkEdit],
) -> Result<HashMap<usize, String>, String> {
    let editable_indices = session
        .chunks
        .iter()
        .filter(|chunk| !chunk.skip_rewrite)
        .map(|chunk| chunk.index)
        .collect::<HashSet<_>>();
    if edits.len() != editable_indices.len() {
        return Err("编辑器提交的可编辑片段数量与当前会话不一致，请重新进入编辑模式。".to_string());
    }

    let mut overrides = HashMap::with_capacity(edits.len());
    for edit in edits {
        if !editable_indices.contains(&edit.index) {
            return Err(format!(
                "编辑器提交了不可编辑或不存在的片段 #{}, 无法安全写回。",
                edit.index + 1
            ));
        }
        if overrides.insert(edit.index, edit.text.clone()).is_some() {
            return Err(format!(
                "编辑器提交了重复的片段 #{}, 无法安全写回。",
                edit.index + 1
            ));
        }
    }

    Ok(overrides)
}

#[cfg(test)]
#[path = "editor_writeback_tests.rs"]
mod tests;
