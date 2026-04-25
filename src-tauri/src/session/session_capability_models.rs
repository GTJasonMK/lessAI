use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum DocumentBackendKind {
    #[default]
    Textual,
    Docx,
    Pdf,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum DocumentEditorMode {
    #[default]
    None,
    FullText,
    SlotBased,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityGate {
    #[serde(default)]
    pub allowed: bool,
    #[serde(default)]
    pub block_reason: Option<String>,
}

impl CapabilityGate {
    pub(crate) fn allowed() -> Self {
        Self {
            allowed: true,
            block_reason: None,
        }
    }

    pub(crate) fn blocked(reason: impl Into<String>) -> Self {
        Self {
            allowed: false,
            block_reason: Some(reason.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSessionCapabilities {
    #[serde(default)]
    pub backend_kind: DocumentBackendKind,
    #[serde(default)]
    pub editor_mode: DocumentEditorMode,
    #[serde(default)]
    pub clean_session: bool,
    #[serde(default)]
    pub source_writeback: CapabilityGate,
    #[serde(default)]
    pub ai_rewrite: CapabilityGate,
    #[serde(default)]
    #[serde(alias = "plainTextEditor")]
    pub editor_writeback: CapabilityGate,
    #[serde(default)]
    pub editor_entry: CapabilityGate,
}
