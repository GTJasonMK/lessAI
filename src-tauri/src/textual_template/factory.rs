#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextualTemplateFormat {
    PlainText,
    Markdown,
    Tex,
}

use super::TextTemplate;

pub(crate) fn build_template(
    source_text: &str,
    format: TextualTemplateFormat,
    rewrite_headings: bool,
) -> TextTemplate {
    match format {
        TextualTemplateFormat::PlainText => {
            crate::adapters::plain_text::PlainTextAdapter::build_template(source_text)
        }
        TextualTemplateFormat::Markdown => {
            crate::adapters::markdown::MarkdownAdapter::build_template(
                source_text,
                rewrite_headings,
            )
        }
        TextualTemplateFormat::Tex => {
            crate::adapters::tex::TexAdapter::build_template(source_text, rewrite_headings)
        }
    }
}
