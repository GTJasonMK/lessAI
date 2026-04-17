pub mod docx;
pub mod markdown;
pub mod pdf;
pub mod tex;

use crate::models::TextPresentation;

#[derive(Debug, Clone)]
pub struct TextRegion {
    pub body: String,
    pub skip_rewrite: bool,
    pub presentation: Option<TextPresentation>,
}
