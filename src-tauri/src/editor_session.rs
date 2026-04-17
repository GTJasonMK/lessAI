use std::path::Path;

use crate::{
    document_snapshot::{
        ensure_document_snapshot_matches, SNAPSHOT_MISMATCH_ERROR, SNAPSHOT_MISSING_ERROR,
    },
    models::DocumentSnapshot,
};

pub(crate) const EDITOR_BASE_SNAPSHOT_MISSING_ERROR: &str =
    "当前编辑器缺少打开时的文件快照，无法确认保存安全性。请重新进入编辑模式后再试。";
pub(crate) const EDITOR_BASE_SNAPSHOT_EXPIRED_ERROR: &str =
    "编辑器基准已过期，原文件已在外部发生变化。请重新进入编辑模式后再试。";
pub(crate) const ACTIVE_EDITOR_SESSION_ERROR: &str =
    "当前文档正在执行自动任务，请先暂停并取消后再继续编辑。";

pub(crate) fn ensure_editor_base_snapshot_matches_path(
    path: &Path,
    editor_base_snapshot: Option<&DocumentSnapshot>,
) -> Result<(), String> {
    match ensure_document_snapshot_matches(path, editor_base_snapshot) {
        Ok(_) => Ok(()),
        Err(error) if error == SNAPSHOT_MISSING_ERROR => {
            Err(EDITOR_BASE_SNAPSHOT_MISSING_ERROR.to_string())
        }
        Err(error) if error == SNAPSHOT_MISMATCH_ERROR => {
            Err(EDITOR_BASE_SNAPSHOT_EXPIRED_ERROR.to_string())
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
#[path = "editor_session_tests.rs"]
mod tests;
