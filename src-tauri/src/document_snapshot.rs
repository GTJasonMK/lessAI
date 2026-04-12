use std::{fs, path::Path};

use sha2::{Digest, Sha256};

use crate::models::DocumentSnapshot;

const SNAPSHOT_MISSING_ERROR: &str =
    "当前会话缺少原文件快照，无法确认写回安全性。请重新导入文档后再写回。";
const SNAPSHOT_MISMATCH_ERROR: &str = "原文件已在外部发生变化。为避免误写，请重新导入。";

pub(crate) fn capture_document_snapshot(path: &Path) -> Result<DocumentSnapshot, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    Ok(snapshot_from_bytes(&bytes))
}

pub(crate) fn ensure_document_snapshot_matches(
    path: &Path,
    expected_snapshot: Option<&DocumentSnapshot>,
) -> Result<Vec<u8>, String> {
    let expected = expected_snapshot.ok_or_else(|| SNAPSHOT_MISSING_ERROR.to_string())?;
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let current = snapshot_from_bytes(&bytes);
    if current != *expected {
        return Err(SNAPSHOT_MISMATCH_ERROR.to_string());
    }
    Ok(bytes)
}

fn snapshot_from_bytes(bytes: &[u8]) -> DocumentSnapshot {
    let digest = Sha256::digest(bytes);
    let sha256 = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    DocumentSnapshot { sha256 }
}
