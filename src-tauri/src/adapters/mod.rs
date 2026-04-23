pub mod docx;
pub mod markdown;
pub mod pdf;
pub mod plain_text;
pub mod tex;

use crate::models::TextPresentation;
use crate::rewrite_unit::WritebackSlotRole;
use crate::textual_template::models::TextRegionSplitMode;

#[derive(Debug, Clone)]
pub struct TextRegion {
    pub body: String,
    pub skip_rewrite: bool,
    pub role: WritebackSlotRole,
    pub split_mode: TextRegionSplitMode,
    pub presentation: Option<TextPresentation>,
}

impl TextRegion {
    pub fn editable(body: impl Into<String>) -> Self {
        Self {
            body: body.into(),
            skip_rewrite: false,
            role: WritebackSlotRole::EditableText,
            split_mode: TextRegionSplitMode::BoundaryAware,
            presentation: None,
        }
    }

    pub fn locked_text(body: impl Into<String>) -> Self {
        Self {
            body: body.into(),
            skip_rewrite: true,
            role: WritebackSlotRole::LockedText,
            split_mode: TextRegionSplitMode::Atomic,
            presentation: None,
        }
    }

    pub fn syntax_token(body: impl Into<String>) -> Self {
        Self {
            role: WritebackSlotRole::SyntaxToken,
            ..Self::locked_text(body)
        }
    }

    pub fn inline_object(body: impl Into<String>) -> Self {
        Self {
            role: WritebackSlotRole::InlineObject,
            ..Self::locked_text(body)
        }
    }

    pub fn with_presentation(mut self, presentation: Option<TextPresentation>) -> Self {
        self.presentation = presentation;
        self
    }

    pub fn with_split_mode(mut self, split_mode: TextRegionSplitMode) -> Self {
        self.split_mode = split_mode;
        self
    }
}
