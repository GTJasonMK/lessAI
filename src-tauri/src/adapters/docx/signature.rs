use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{
    rewrite_unit::WritebackSlot,
    textual_template::signature::compute_slot_structure_signature,
};

use super::{
    model::WritebackBlockTemplate,
    slots::{build_writeback_slots, locked_role, paragraph_is_locked, region_role},
};

#[derive(Debug, Clone)]
pub(crate) struct DocxWritebackModel {
    pub source_text: String,
    pub writeback_slots: Vec<WritebackSlot>,
    pub template_signature: String,
    pub slot_structure_signature: String,
}

pub(super) fn build_docx_writeback_model(
    blocks: &[WritebackBlockTemplate],
    rewrite_headings: bool,
) -> DocxWritebackModel {
    let writeback_slots = build_writeback_slots(blocks, rewrite_headings);
    let source_text = writeback_slots
        .iter()
        .map(|slot| format!("{}{}", slot.text, slot.separator_after))
        .collect::<String>();

    DocxWritebackModel {
        source_text,
        template_signature: compute_docx_template_signature(blocks, rewrite_headings),
        slot_structure_signature: compute_slot_structure_signature(&writeback_slots),
        writeback_slots,
    }
}

fn compute_docx_template_signature(
    blocks: &[WritebackBlockTemplate],
    rewrite_headings: bool,
) -> String {
    let normalized = blocks
        .iter()
        .enumerate()
        .map(|(block_index, block)| match block {
            WritebackBlockTemplate::Paragraph(paragraph) => (
                "paragraph",
                format!("docx:p{block_index}"),
                paragraph.is_heading,
                paragraph
                    .regions
                    .iter()
                    .enumerate()
                    .map(|(region_index, region)| {
                        let editable = !paragraph_is_locked(paragraph, region, rewrite_headings);
                        (
                            format!("docx:p{block_index}:r{region_index}"),
                            region.text().to_string(),
                            editable,
                            region_role(region, editable),
                            region.presentation().cloned(),
                        )
                    })
                    .collect::<Vec<_>>(),
            ),
            WritebackBlockTemplate::Locked(region) => (
                "locked_block",
                format!("docx:block:{block_index}"),
                false,
                vec![(
                    format!("docx:block:{block_index}:r0"),
                    region.text.clone(),
                    false,
                    locked_role(region.presentation.as_ref()),
                    region.presentation.clone(),
                )],
            ),
        })
        .collect::<Vec<_>>();
    signature_hex(&normalized)
}

fn signature_hex<T>(value: &T) -> String
where
    T: Serialize,
{
    let bytes = serde_json::to_vec(value).expect("serialize docx signature payload");
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}
