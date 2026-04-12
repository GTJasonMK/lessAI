use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    #[serde(default)]
    pub update_proxy: String,
    pub timeout_ms: u64,
    pub temperature: f32,
    pub chunk_preset: ChunkPreset,
    #[serde(default)]
    pub rewrite_headings: bool,
    pub rewrite_mode: RewriteMode,
    #[serde(default = "default_max_concurrency")]
    pub max_concurrency: usize,
    #[serde(default = "default_prompt_preset_id")]
    pub prompt_preset_id: String,
    #[serde(default)]
    pub custom_prompts: Vec<PromptTemplate>,
}

fn default_max_concurrency() -> usize {
    2
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
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: String::new(),
            model: "gpt-4.1-mini".to_string(),
            update_proxy: String::new(),
            timeout_ms: 45_000,
            temperature: 0.8,
            chunk_preset: ChunkPreset::Paragraph,
            rewrite_headings: false,
            rewrite_mode: RewriteMode::Manual,
            max_concurrency: default_max_concurrency(),
            prompt_preset_id: default_prompt_preset_id(),
            custom_prompts: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ChunkPreset {
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
pub enum ChunkStatus {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffSpan {
    pub r#type: DiffType,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChunkPresentation {
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
pub struct ChunkTask {
    pub index: usize,
    pub source_text: String,
    pub separator_after: String,
    #[serde(default)]
    pub skip_rewrite: bool,
    #[serde(default)]
    pub presentation: Option<ChunkPresentation>,
    pub status: ChunkStatus,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorChunkEdit {
    pub index: usize,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditSuggestion {
    pub id: String,
    pub sequence: u64,
    pub chunk_index: usize,
    pub before_text: String,
    pub after_text: String,
    pub diff_spans: Vec<DiffSpan>,
    pub decision: SuggestionDecision,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
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
    pub chunk_preset: Option<ChunkPreset>,
    #[serde(default)]
    pub rewrite_headings: Option<bool>,
    pub chunks: Vec<ChunkTask>,
    pub suggestions: Vec<EditSuggestion>,
    pub next_suggestion_sequence: u64,
    pub status: RunningState,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
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
    pub completed_chunks: usize,
    pub in_flight: usize,
    pub running_indices: Vec<usize>,
    pub total_chunks: usize,
    pub mode: RewriteMode,
    pub running_state: RunningState,
    pub max_concurrency: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChunkCompletedEvent {
    pub session_id: String,
    pub index: usize,
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
mod tests {
    use super::ChunkPreset;

    #[test]
    fn rejects_legacy_chunk_preset_aliases() {
        for legacy in ["small", "medium", "large", "question"] {
            let payload = format!("\"{legacy}\"");
            let parsed = serde_json::from_str::<ChunkPreset>(&payload);
            assert!(
                parsed.is_err(),
                "legacy preset should be rejected: {legacy}"
            );
        }
    }

    #[test]
    fn accepts_current_chunk_preset_values() {
        assert_eq!(
            serde_json::from_str::<ChunkPreset>("\"clause\"").unwrap(),
            ChunkPreset::Clause
        );
        assert_eq!(
            serde_json::from_str::<ChunkPreset>("\"sentence\"").unwrap(),
            ChunkPreset::Sentence
        );
        assert_eq!(
            serde_json::from_str::<ChunkPreset>("\"paragraph\"").unwrap(),
            ChunkPreset::Paragraph
        );
    }
}
