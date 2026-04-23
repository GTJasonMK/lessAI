use crate::{
    rewrite_unit::WritebackSlotRole,
    text_boundaries::{
        split_text_and_trailing_separator, split_text_chunks_by_paragraph_separator,
    },
    textual_template::{
        models::{TextRegionSplitMode, TextTemplateBlock, TextTemplateRegion},
        TextTemplate,
    },
};

pub struct PlainTextAdapter;

impl PlainTextAdapter {
    pub fn build_template(text: &str) -> TextTemplate {
        if text.is_empty() {
            return TextTemplate::new("plain_text", Vec::new());
        }

        let blocks = split_text_chunks_by_paragraph_separator(text)
            .into_iter()
            .enumerate()
            .map(|(paragraph_index, chunk)| build_paragraph_block(paragraph_index, chunk))
            .collect::<Vec<_>>();

        TextTemplate::new("plain_text", blocks)
    }
}

fn build_paragraph_block(paragraph_index: usize, chunk: &str) -> TextTemplateBlock {
    let (text, separator_after) = split_text_and_trailing_separator(chunk);
    let anchor = format!("txt:p{paragraph_index}");

    TextTemplateBlock {
        anchor: anchor.clone(),
        kind: "paragraph".to_string(),
        regions: vec![TextTemplateRegion {
            anchor: format!("{anchor}:r0"),
            text,
            editable: true,
            role: WritebackSlotRole::EditableText,
            presentation: None,
            split_mode: TextRegionSplitMode::BoundaryAware,
            separator_after,
        }],
    }
}
