use std::path::Path;

use crate::{
    models::{DocumentSession, RewriteUnitStatus, RunningState},
    session_capability_models::{
        CapabilityGate, DocumentBackendKind, DocumentEditorMode, DocumentSessionCapabilities,
    },
};

use super::textual::path_extension_lower;

pub(crate) const DIRTY_SESSION_BLOCK_REASON: &str =
    "该文档存在修订记录或进度，为避免冲突，请先“覆写并清理记录”或“重置记录”后再编辑。";
const AI_REWRITE_BLOCK_REASON: &str = "当前文档暂不支持安全写回覆盖，因此不允许继续 AI 改写。";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DocumentCapabilityPolicy {
    pub(crate) source_writeback: CapabilityGate,
    pub(crate) editor_writeback: CapabilityGate,
}

impl DocumentCapabilityPolicy {
    pub(crate) fn new(
        source_writeback: CapabilityGate,
        editor_writeback: CapabilityGate,
    ) -> Self {
        Self {
            source_writeback,
            editor_writeback,
        }
    }
}

pub(crate) fn document_backend_kind(path: &Path) -> DocumentBackendKind {
    match path_extension_lower(path).as_deref() {
        Some("docx") => DocumentBackendKind::Docx,
        Some("pdf") => DocumentBackendKind::Pdf,
        _ => DocumentBackendKind::Textual,
    }
}

pub(crate) fn session_document_backend(session: &DocumentSession) -> DocumentBackendKind {
    document_backend_kind(Path::new(&session.document_path))
}

pub(crate) fn ensure_capability_allowed(
    gate: &CapabilityGate,
    default_message: &str,
) -> Result<(), String> {
    if gate.allowed {
        return Ok(());
    }

    Err(gate
        .block_reason
        .clone()
        .unwrap_or_else(|| default_message.to_string()))
}

pub(crate) fn capability_gate(allowed: bool, block_reason: Option<&str>) -> CapabilityGate {
    if allowed {
        CapabilityGate::allowed()
    } else {
        CapabilityGate::blocked(
            block_reason
                .unwrap_or("当前文档能力状态不一致，缺少阻断原因。"),
        )
    }
}

pub(crate) fn hydrate_session_capabilities(session: &mut DocumentSession) {
    session.capabilities = build_session_capabilities(session);
}

pub(crate) fn hydrated_session_clone(session: &DocumentSession) -> DocumentSession {
    let mut cloned = session.clone();
    hydrate_session_capabilities(&mut cloned);
    cloned
}

pub(crate) fn apply_capability_policy(
    session: &mut DocumentSession,
    policy: &DocumentCapabilityPolicy,
) -> bool {
    let source_writeback_changed = session.capabilities.source_writeback != policy.source_writeback;
    let editor_writeback_changed =
        session.capabilities.editor_writeback != policy.editor_writeback;
    if !source_writeback_changed && !editor_writeback_changed {
        return false;
    }

    session.capabilities.source_writeback = policy.source_writeback.clone();
    session.capabilities.editor_writeback = policy.editor_writeback.clone();
    hydrate_session_capabilities(session);
    true
}

fn build_session_capabilities(session: &DocumentSession) -> DocumentSessionCapabilities {
    let backend_kind = session_document_backend(session);
    let source_writeback = normalized_policy_gate(&session.capabilities.source_writeback);
    let editor_writeback = normalized_policy_gate(&session.capabilities.editor_writeback);
    let editor_mode = compute_document_editor_mode(
        backend_kind,
        session.template_kind.as_deref(),
        editor_writeback.allowed,
    );
    let clean_session = compute_clean_session(session);

    DocumentSessionCapabilities {
        backend_kind,
        editor_mode,
        clean_session,
        source_writeback: source_writeback.clone(),
        ai_rewrite: ai_rewrite_gate(backend_kind, &source_writeback),
        editor_writeback: editor_writeback.clone(),
        editor_entry: editor_entry_gate(editor_mode, clean_session, &editor_writeback),
    }
}

fn normalized_policy_gate(gate: &CapabilityGate) -> CapabilityGate {
    if gate.allowed || gate.block_reason.is_some() {
        return gate.clone();
    }

    CapabilityGate::allowed()
}

fn compute_clean_session(session: &DocumentSession) -> bool {
    session.status == RunningState::Idle
        && session.suggestions.is_empty()
        && session.rewrite_units.iter().all(|unit| {
            matches!(unit.status, RewriteUnitStatus::Idle | RewriteUnitStatus::Done)
        })
}

fn ai_rewrite_gate(
    backend_kind: DocumentBackendKind,
    source_writeback: &CapabilityGate,
) -> CapabilityGate {
    match backend_kind {
        DocumentBackendKind::Pdf => CapabilityGate::allowed(),
        DocumentBackendKind::Docx | DocumentBackendKind::Textual => {
            if source_writeback.allowed {
                CapabilityGate::allowed()
            } else {
                CapabilityGate::blocked(
                    source_writeback
                        .block_reason
                        .as_deref()
                        .unwrap_or(AI_REWRITE_BLOCK_REASON),
                )
            }
        }
    }
}

fn editor_entry_gate(
    editor_mode: DocumentEditorMode,
    clean_session: bool,
    editor_writeback: &CapabilityGate,
) -> CapabilityGate {
    if editor_mode == DocumentEditorMode::None {
        return CapabilityGate::blocked(
            editor_writeback
                .block_reason
                .as_deref()
                .unwrap_or("当前文档暂不支持进入编辑模式。"),
        );
    }
    if !editor_writeback.allowed {
        return CapabilityGate::blocked(
            editor_writeback
                .block_reason
                .as_deref()
                .unwrap_or("当前文档暂不支持进入编辑模式。"),
        );
    }
    if !clean_session {
        return CapabilityGate::blocked(DIRTY_SESSION_BLOCK_REASON);
    }
    CapabilityGate::allowed()
}

fn compute_document_editor_mode(
    backend_kind: DocumentBackendKind,
    template_kind: Option<&str>,
    editor_writeback_safe: bool,
) -> DocumentEditorMode {
    if !editor_writeback_safe {
        return DocumentEditorMode::None;
    }

    match backend_kind {
        DocumentBackendKind::Pdf => DocumentEditorMode::None,
        DocumentBackendKind::Docx => DocumentEditorMode::SlotBased,
        DocumentBackendKind::Textual => match template_kind {
            Some("markdown" | "tex") => DocumentEditorMode::SlotBased,
            _ => DocumentEditorMode::FullText,
        },
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use std::path::Path;

    use crate::{
        models::{DocumentSession, RunningState, SegmentationPreset},
        session_capability_models::{CapabilityGate, DocumentSessionCapabilities},
    };

    fn sample_session(document_path: &str) -> DocumentSession {
        let now = Utc::now();
        let mut session = DocumentSession {
            id: "session-1".to_string(),
            title: "示例".to_string(),
            document_path: document_path.to_string(),
            source_text: "正文".to_string(),
            source_snapshot: None,
            template_kind: None,
            template_signature: None,
            slot_structure_signature: None,
            template_snapshot: None,
            normalized_text: "正文".to_string(),
            capabilities: DocumentSessionCapabilities {
                source_writeback: CapabilityGate::allowed(),
                editor_writeback: CapabilityGate::allowed(),
                ..Default::default()
            },
            segmentation_preset: Some(SegmentationPreset::Paragraph),
            rewrite_headings: Some(false),
            writeback_slots: Vec::new(),
            rewrite_units: Vec::new(),
            suggestions: Vec::new(),
            next_suggestion_sequence: 1,
            status: RunningState::Idle,
            created_at: now,
            updated_at: now,
        };
        super::hydrate_session_capabilities(&mut session);
        session
    }

    #[test]
    fn document_backend_kind_uses_extension_groups() {
        assert_eq!(
            super::document_backend_kind(Path::new("/tmp/example.txt")),
            crate::session_capability_models::DocumentBackendKind::Textual
        );
        assert_eq!(
            super::document_backend_kind(Path::new("/tmp/example.md")),
            crate::session_capability_models::DocumentBackendKind::Textual
        );
        assert_eq!(
            super::document_backend_kind(Path::new("/tmp/example.docx")),
            crate::session_capability_models::DocumentBackendKind::Docx
        );
        assert_eq!(
            super::document_backend_kind(Path::new("/tmp/example.pdf")),
            crate::session_capability_models::DocumentBackendKind::Pdf
        );
    }

    #[test]
    fn hydrate_session_capabilities_keeps_pdf_ai_rewrite_enabled() {
        let pdf = sample_session("/tmp/example.pdf");
        assert!(pdf.capabilities.ai_rewrite.allowed);

        let mut docx = sample_session("/tmp/example.docx");
        docx.capabilities.source_writeback = CapabilityGate::blocked("blocked");
        super::hydrate_session_capabilities(&mut docx);
        assert!(!docx.capabilities.ai_rewrite.allowed);

        let textual = sample_session("/tmp/example.txt");
        assert!(textual.capabilities.ai_rewrite.allowed);
    }

    #[test]
    fn hydrate_session_capabilities_tracks_backend_and_editor_safety() {
        let textual = sample_session("/tmp/example.txt");
        assert_eq!(
            textual.capabilities.editor_mode,
            crate::session_capability_models::DocumentEditorMode::FullText
        );

        let mut markdown = sample_session("/tmp/example.md");
        markdown.template_kind = Some("markdown".to_string());
        super::hydrate_session_capabilities(&mut markdown);
        assert_eq!(
            markdown.capabilities.editor_mode,
            crate::session_capability_models::DocumentEditorMode::SlotBased
        );

        let mut tex = sample_session("/tmp/example.tex");
        tex.template_kind = Some("tex".to_string());
        super::hydrate_session_capabilities(&mut tex);
        assert_eq!(
            tex.capabilities.editor_mode,
            crate::session_capability_models::DocumentEditorMode::SlotBased
        );

        let docx = sample_session("/tmp/example.docx");
        assert_eq!(
            docx.capabilities.editor_mode,
            crate::session_capability_models::DocumentEditorMode::SlotBased
        );

        let mut pdf = sample_session("/tmp/example.pdf");
        assert_eq!(
            pdf.capabilities.editor_mode,
            crate::session_capability_models::DocumentEditorMode::None
        );

        pdf.document_path = "/tmp/example.docx".to_string();
        pdf.capabilities.editor_writeback = CapabilityGate::blocked("blocked");
        super::hydrate_session_capabilities(&mut pdf);
        assert_eq!(
            pdf.capabilities.editor_mode,
            crate::session_capability_models::DocumentEditorMode::None
        );
    }

    #[test]
    fn hydrate_session_capabilities_marks_dirty_editor_entry() {
        let mut session = sample_session("/tmp/example.md");
        session.template_kind = Some("markdown".to_string());
        session.suggestions.push(crate::rewrite_unit::RewriteSuggestion {
            id: "s-1".to_string(),
            sequence: 1,
            rewrite_unit_id: "unit-1".to_string(),
            before_text: "原文".to_string(),
            after_text: "改文".to_string(),
            diff: crate::models::DiffResult::default(),
            decision: crate::models::SuggestionDecision::Proposed,
            slot_updates: Vec::new(),
            created_at: session.created_at,
            updated_at: session.updated_at,
        });

        super::hydrate_session_capabilities(&mut session);

        assert!(!session.capabilities.clean_session);
        assert!(!session.capabilities.editor_entry.allowed);
        assert_eq!(
            session.capabilities.editor_entry.block_reason.as_deref(),
            Some(super::DIRTY_SESSION_BLOCK_REASON)
        );
    }
}
