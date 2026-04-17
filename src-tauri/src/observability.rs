use std::path::Path;

use crate::{
    documents::WritebackMode,
    models::{RewriteMode, RunningState},
};

pub(crate) fn rewrite_mode_label(mode: RewriteMode) -> &'static str {
    match mode {
        RewriteMode::Manual => "manual",
        RewriteMode::Auto => "auto",
    }
}

pub(crate) fn running_state_label(state: RunningState) -> &'static str {
    match state {
        RunningState::Idle => "idle",
        RunningState::Running => "running",
        RunningState::Paused => "paused",
        RunningState::Completed => "completed",
        RunningState::Cancelled => "cancelled",
        RunningState::Failed => "failed",
    }
}

pub(crate) fn writeback_mode_label(mode: WritebackMode) -> &'static str {
    match mode {
        WritebackMode::Validate => "validate",
        WritebackMode::Write => "write",
    }
}

pub(crate) fn target_rewrite_unit_ids_label(rewrite_unit_ids: Option<&[String]>) -> String {
    match rewrite_unit_ids {
        None => "all".to_string(),
        Some(rewrite_unit_ids) => {
            let joined = rewrite_unit_ids
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(",");
            format!("[{joined}]")
        }
    }
}

pub(crate) fn document_kind_label(path: &str) -> &'static str {
    if extension_is(path, "docx") {
        return "docx";
    }
    if extension_is(path, "pdf") {
        return "pdf";
    }
    "text"
}

fn extension_is(path: &str, expected: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case(expected))
}

#[cfg(test)]
mod tests {
    use crate::{
        documents::WritebackMode,
        models::{RewriteMode, RunningState},
    };

    #[test]
    fn rewrite_mode_label_matches_expected_names() {
        assert_eq!(super::rewrite_mode_label(RewriteMode::Manual), "manual");
        assert_eq!(super::rewrite_mode_label(RewriteMode::Auto), "auto");
    }

    #[test]
    fn running_state_label_matches_expected_names() {
        assert_eq!(super::running_state_label(RunningState::Idle), "idle");
        assert_eq!(super::running_state_label(RunningState::Running), "running");
        assert_eq!(super::running_state_label(RunningState::Paused), "paused");
        assert_eq!(
            super::running_state_label(RunningState::Completed),
            "completed"
        );
        assert_eq!(
            super::running_state_label(RunningState::Cancelled),
            "cancelled"
        );
        assert_eq!(super::running_state_label(RunningState::Failed), "failed");
    }

    #[test]
    fn writeback_mode_label_matches_expected_names() {
        assert_eq!(
            super::writeback_mode_label(WritebackMode::Validate),
            "validate"
        );
        assert_eq!(super::writeback_mode_label(WritebackMode::Write), "write");
    }

    #[test]
    fn target_rewrite_unit_ids_label_formats_none_and_values() {
        assert_eq!(super::target_rewrite_unit_ids_label(None), "all");
        assert_eq!(super::target_rewrite_unit_ids_label(Some(&[])), "[]");
        assert_eq!(
            super::target_rewrite_unit_ids_label(Some(&[
                "unit-1".to_string(),
                "unit-3".to_string()
            ])),
            "[unit-1,unit-3]"
        );
    }

    #[test]
    fn document_kind_label_uses_extension() {
        assert_eq!(super::document_kind_label("C:/docs/file.docx"), "docx");
        assert_eq!(super::document_kind_label("C:/docs/file.pdf"), "pdf");
        assert_eq!(super::document_kind_label("C:/docs/file.md"), "text");
    }
}
