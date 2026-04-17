use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::models::{
    TextPresentation, SegmentationPreset, RewriteUnitStatus, DiffSpan, SuggestionDecision,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RewriteSuggestion {
    pub id: String,
    pub sequence: u64,
    pub rewrite_unit_id: String,
    pub before_text: String,
    pub after_text: String,
    pub diff_spans: Vec<DiffSpan>,
    pub decision: SuggestionDecision,
    pub slot_updates: Vec<SlotUpdate>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
