use serde::{Deserialize, Serialize};

use crate::{models::TextPresentation, rewrite_unit::WritebackSlotRole};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum TextRegionSplitMode {
    #[default]
    BoundaryAware,
    Atomic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TextTemplate {
    pub kind: String,
    pub blocks: Vec<TextTemplateBlock>,
    pub template_signature: String,
}

impl TextTemplate {
    pub(crate) fn new(kind: &str, blocks: Vec<TextTemplateBlock>) -> Self {
        Self {
            kind: kind.to_string(),
            template_signature: super::signature::compute_template_signature(kind, &blocks),
            blocks,
        }
    }

    #[cfg(test)]
    pub(crate) fn single_paragraph(kind: &str, block_anchor: &str, text: &str) -> Self {
        let (body, separator_after) =
            crate::text_boundaries::split_text_and_trailing_separator(text);
        Self::new(
            kind,
            vec![TextTemplateBlock {
                anchor: block_anchor.to_string(),
                kind: "paragraph".to_string(),
                regions: vec![TextTemplateRegion {
                    anchor: format!("{block_anchor}:r0"),
                    text: body,
                    editable: true,
                    role: WritebackSlotRole::EditableText,
                    presentation: None,
                    split_mode: TextRegionSplitMode::BoundaryAware,
                    separator_after,
                }],
            }],
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TextTemplateBlock {
    pub anchor: String,
    pub kind: String,
    pub regions: Vec<TextTemplateRegion>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TextTemplateRegion {
    pub anchor: String,
    pub text: String,
    pub editable: bool,
    pub role: WritebackSlotRole,
    pub presentation: Option<TextPresentation>,
    #[serde(default)]
    pub split_mode: TextRegionSplitMode,
    #[serde(default)]
    pub separator_after: String,
}
