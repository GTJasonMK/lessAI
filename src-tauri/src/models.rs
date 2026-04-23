use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};

use crate::session_capability_models::{
    CapabilityGate, DocumentSessionCapabilities,
};
use crate::rewrite_unit::{RewriteSuggestion, RewriteUnit, WritebackSlot};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppSettings {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub update_proxy: String,
    pub timeout_ms: u64,
    pub temperature: f32,
    pub segmentation_preset: SegmentationPreset,
    pub rewrite_headings: bool,
    pub rewrite_mode: RewriteMode,
    pub max_concurrency: usize,
    pub units_per_batch: usize,
    pub prompt_preset_id: String,
    pub custom_prompts: Vec<PromptTemplate>,
}

fn default_max_concurrency() -> usize {
    2
}

fn default_units_per_batch() -> usize {
    1
}

fn default_prompt_preset_id() -> String {
    "humanizer_zh".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptTemplate {
    pub id: String,
    pub name: String,
    pub content: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: String::new(),
            model: "gpt-4.1-mini".to_string(),
            update_proxy: String::new(),
            timeout_ms: 45_000,
            temperature: 0.8,
            segmentation_preset: SegmentationPreset::Paragraph,
            rewrite_headings: false,
            rewrite_mode: RewriteMode::Manual,
            max_concurrency: default_max_concurrency(),
            units_per_batch: default_units_per_batch(),
            prompt_preset_id: default_prompt_preset_id(),
            custom_prompts: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SegmentationPreset {
    Clause,
    Sentence,
    Paragraph,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DocumentFormat {
    PlainText,
    Markdown,
    Tex,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RewriteMode {
    Manual,
    Auto,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RewriteUnitStatus {
    Idle,
    Running,
    Done,
    Failed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SuggestionDecision {
    Proposed,
    Applied,
    Dismissed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DiffType {
    Unchanged,
    Insert,
    Delete,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RunningState {
    Idle,
    Running,
    Paused,
    Completed,
    Cancelled,
    Failed,
}

impl RunningState {
    pub(crate) fn is_active_job(self) -> bool {
        matches!(self, Self::Running | Self::Paused)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffSpan {
    pub r#type: DiffType,
    pub text: String,
    #[serde(default)]
    pub degraded_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DiffResult {
    #[serde(default)]
    pub spans: Vec<DiffSpan>,
    #[serde(default)]
    pub degraded_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TextPresentation {
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub underline: bool,
    #[serde(default)]
    pub href: Option<String>,
    #[serde(default)]
    pub protect_kind: Option<String>,
    #[serde(default)]
    pub writeback_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSnapshot {
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorSlotEdit {
    pub slot_id: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSession {
    pub id: String,
    pub title: String,
    pub document_path: String,
    pub source_text: String,
    #[serde(default)]
    pub source_snapshot: Option<DocumentSnapshot>,
    #[serde(default)]
    pub template_kind: Option<String>,
    #[serde(default)]
    pub template_signature: Option<String>,
    #[serde(default)]
    pub slot_structure_signature: Option<String>,
    #[serde(default)]
    pub template_snapshot: Option<crate::textual_template::TextTemplate>,
    pub normalized_text: String,
    #[serde(default)]
    pub capabilities: DocumentSessionCapabilities,
    #[serde(default)]
    pub segmentation_preset: Option<SegmentationPreset>,
    #[serde(default)]
    pub rewrite_headings: Option<bool>,
    #[serde(default)]
    pub writeback_slots: Vec<WritebackSlot>,
    #[serde(default)]
    pub rewrite_units: Vec<RewriteUnit>,
    pub suggestions: Vec<RewriteSuggestion>,
    pub next_suggestion_sequence: u64,
    pub status: RunningState,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DocumentSessionWire {
    id: String,
    title: String,
    document_path: String,
    source_text: String,
    #[serde(default)]
    source_snapshot: Option<DocumentSnapshot>,
    #[serde(default)]
    template_kind: Option<String>,
    #[serde(default)]
    template_signature: Option<String>,
    #[serde(default)]
    slot_structure_signature: Option<String>,
    #[serde(default)]
    template_snapshot: Option<crate::textual_template::TextTemplate>,
    normalized_text: String,
    #[serde(default)]
    capabilities: DocumentSessionCapabilities,
    #[serde(default)]
    write_back_supported: Option<bool>,
    #[serde(default)]
    write_back_block_reason: Option<String>,
    #[serde(default)]
    #[serde(alias = "plainTextEditorSafe")]
    editor_writeback_safe: Option<bool>,
    #[serde(default)]
    #[serde(alias = "plainTextEditorBlockReason")]
    editor_writeback_block_reason: Option<String>,
    #[serde(default)]
    segmentation_preset: Option<SegmentationPreset>,
    #[serde(default)]
    rewrite_headings: Option<bool>,
    #[serde(default)]
    writeback_slots: Vec<WritebackSlot>,
    #[serde(default)]
    rewrite_units: Vec<RewriteUnit>,
    suggestions: Vec<RewriteSuggestion>,
    next_suggestion_sequence: u64,
    status: RunningState,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl DocumentSessionWire {
    fn into_session(self) -> DocumentSession {
        let capabilities = migrate_session_capabilities(
            self.capabilities,
            self.write_back_supported,
            self.write_back_block_reason,
            self.editor_writeback_safe,
            self.editor_writeback_block_reason,
        );

        DocumentSession {
            id: self.id,
            title: self.title,
            document_path: self.document_path,
            source_text: self.source_text,
            source_snapshot: self.source_snapshot,
            template_kind: self.template_kind,
            template_signature: self.template_signature,
            slot_structure_signature: self.slot_structure_signature,
            template_snapshot: self.template_snapshot,
            normalized_text: self.normalized_text,
            capabilities,
            segmentation_preset: self.segmentation_preset,
            rewrite_headings: self.rewrite_headings,
            writeback_slots: self.writeback_slots,
            rewrite_units: self.rewrite_units,
            suggestions: self.suggestions,
            next_suggestion_sequence: self.next_suggestion_sequence,
            status: self.status,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

impl<'de> Deserialize<'de> for DocumentSession {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        DocumentSessionWire::deserialize(deserializer).map(DocumentSessionWire::into_session)
    }
}

fn migrate_session_capabilities(
    mut capabilities: DocumentSessionCapabilities,
    write_back_supported: Option<bool>,
    write_back_block_reason: Option<String>,
    editor_writeback_safe: Option<bool>,
    editor_writeback_block_reason: Option<String>,
) -> DocumentSessionCapabilities {
    capabilities.source_writeback = merge_legacy_capability_gate(
        capabilities.source_writeback,
        write_back_supported,
        write_back_block_reason,
        true,
    );
    capabilities.editor_writeback = merge_legacy_capability_gate(
        capabilities.editor_writeback,
        editor_writeback_safe,
        editor_writeback_block_reason,
        true,
    );
    capabilities
}

fn merge_legacy_capability_gate(
    current: CapabilityGate,
    legacy_allowed: Option<bool>,
    legacy_block_reason: Option<String>,
    default_allowed: bool,
) -> CapabilityGate {
    if current.allowed || current.block_reason.is_some() {
        return current;
    }

    match legacy_allowed.unwrap_or(default_allowed) {
        true => CapabilityGate::allowed(),
        false => CapabilityGate::blocked(
            legacy_block_reason.unwrap_or_else(|| "当前文档能力状态不一致，缺少阻断原因。".to_string()),
        ),
    }
}

impl DocumentSession {
    pub(crate) fn has_active_job(&self) -> bool {
        self.status.is_active_job()
    }

    pub(crate) fn downgrade_active_job_to_cancelled(&mut self) -> bool {
        if !self.has_active_job() {
            return false;
        }
        self.status = RunningState::Cancelled;
        true
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCheckResult {
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RewriteProgress {
    pub session_id: String,
    pub completed_units: usize,
    pub in_flight: usize,
    pub running_unit_ids: Vec<String>,
    pub total_units: usize,
    pub mode: RewriteMode,
    pub running_state: RunningState,
    pub max_concurrency: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RewriteUnitCompletedEvent {
    pub session_id: String,
    pub rewrite_unit_id: String,
    pub suggestion_id: String,
    pub suggestion_sequence: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RewriteFailedEvent {
    pub session_id: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionEvent {
    pub session_id: String,
}

#[cfg(test)]
#[path = "models_tests.rs"]
mod tests;
