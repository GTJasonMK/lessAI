use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};

use crate::models::{
    DiffResult, DiffSpan, RewriteUnitStatus, SegmentationPreset, SuggestionDecision,
    TextPresentation,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum WritebackSlotRole {
    EditableText,
    LockedText,
    SyntaxToken,
    InlineObject,
    ParagraphBreak,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WritebackSlot {
    pub id: String,
    pub order: usize,
    pub text: String,
    pub editable: bool,
    pub role: WritebackSlotRole,
    #[serde(default)]
    pub presentation: Option<TextPresentation>,
    #[serde(default)]
    pub anchor: Option<String>,
    #[serde(default)]
    pub separator_after: String,
}

impl WritebackSlot {
    #[cfg(test)]
    pub(crate) fn editable(id: &str, order: usize, text: &str) -> Self {
        Self {
            id: id.to_string(),
            order,
            text: text.to_string(),
            editable: true,
            role: WritebackSlotRole::EditableText,
            presentation: None,
            anchor: None,
            separator_after: String::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn locked(id: &str, order: usize, text: &str) -> Self {
        Self {
            id: id.to_string(),
            order,
            text: text.to_string(),
            editable: false,
            role: WritebackSlotRole::LockedText,
            presentation: None,
            anchor: None,
            separator_after: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RewriteUnit {
    pub id: String,
    pub order: usize,
    pub slot_ids: Vec<String>,
    pub display_text: String,
    pub segmentation_preset: SegmentationPreset,
    pub status: RewriteUnitStatus,
    #[serde(default)]
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SlotUpdate {
    pub slot_id: String,
    pub text: String,
}

impl SlotUpdate {
    pub(crate) fn new(slot_id: &str, text: &str) -> Self {
        Self {
            slot_id: slot_id.to_string(),
            text: text.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RewriteSuggestion {
    pub id: String,
    pub sequence: u64,
    pub rewrite_unit_id: String,
    pub before_text: String,
    pub after_text: String,
    pub diff: DiffResult,
    pub decision: SuggestionDecision,
    pub slot_updates: Vec<SlotUpdate>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RewriteSuggestionWire {
    id: String,
    sequence: u64,
    rewrite_unit_id: String,
    before_text: String,
    after_text: String,
    #[serde(default)]
    diff: Option<DiffResult>,
    #[serde(default)]
    diff_spans: Vec<DiffSpan>,
    decision: SuggestionDecision,
    #[serde(default)]
    slot_updates: Vec<SlotUpdate>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl RewriteSuggestionWire {
    fn into_suggestion(self) -> RewriteSuggestion {
        let diff = self.diff.unwrap_or_else(|| DiffResult {
            degraded_reason: self
                .diff_spans
                .iter()
                .find_map(|span| span.degraded_reason.clone()),
            spans: self.diff_spans,
        });

        RewriteSuggestion {
            id: self.id,
            sequence: self.sequence,
            rewrite_unit_id: self.rewrite_unit_id,
            before_text: self.before_text,
            after_text: self.after_text,
            diff,
            decision: self.decision,
            slot_updates: self.slot_updates,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

impl<'de> Deserialize<'de> for RewriteSuggestion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        RewriteSuggestionWire::deserialize(deserializer).map(RewriteSuggestionWire::into_suggestion)
    }
}
