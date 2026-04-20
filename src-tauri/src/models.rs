use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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

fn default_write_back_supported() -> bool {
    true
}

fn default_plain_text_editor_safe() -> bool {
    true
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
            base_url: "https://cliproxy.eqing.tech/v1".to_string(),
            api_key: String::new(),
            model: "gpt-5.4-mini".to_string(),
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    #[serde(default = "default_write_back_supported")]
    pub write_back_supported: bool,
    #[serde(default)]
    pub write_back_block_reason: Option<String>,
    #[serde(default = "default_plain_text_editor_safe")]
    pub plain_text_editor_safe: bool,
    #[serde(default)]
    pub plain_text_editor_block_reason: Option<String>,
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
