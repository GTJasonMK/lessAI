pub mod docx;
pub mod markdown;
pub mod pdf;
pub mod tex;

#[derive(Debug, Clone)]
pub struct TextRegion {
    pub body: String,
    pub skip_rewrite: bool,
}
