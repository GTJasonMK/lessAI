use crate::{
    models::{AppSettings, DocumentFormat},
    rewrite_unit::{
        apply_slot_updates, build_rewrite_unit_request_from_slots, merged_text_from_slots,
        RewriteUnitResponse, SlotUpdate, WritebackSlot,
    },
    textual_template,
};

use super::plain_support::finalize_plain_candidate;

const SELECTION_REWRITE_UNIT_ID: &str = "selection";

pub(super) async fn rewrite_selection_text_with_client(
    client: &reqwest::Client,
    settings: &AppSettings,
    source_text: &str,
    format: DocumentFormat,
    rewrite_headings: bool,
) -> Result<String, String> {
    super::validate_settings(settings)?;

    let slots = build_selection_slots(source_text, format, rewrite_headings);
    if !slots
        .iter()
        .any(|slot| slot.editable && !slot.text.trim().is_empty())
    {
        return Err("选区不包含可改写文本。".to_string());
    }

    let request = build_rewrite_unit_request_from_slots(SELECTION_REWRITE_UNIT_ID, &slots, format);
    let response = super::rewrite_unit_with_client(client, settings, &request).await?;
    let updates = normalize_selection_updates(&slots, response)?;
    let updated_slots = apply_slot_updates(&slots, &updates)?;
    Ok(merged_text_from_slots(&updated_slots))
}

fn build_selection_slots(
    source_text: &str,
    format: DocumentFormat,
    rewrite_headings: bool,
) -> Vec<WritebackSlot> {
    // Selection rewrite works on in-memory snippet text; for docx/pdf snippets
    // we intentionally use plain-text slot projection.
    let template = match format {
        DocumentFormat::PlainText | DocumentFormat::Docx | DocumentFormat::Pdf => {
            crate::adapters::plain_text::PlainTextAdapter::build_template(source_text)
        }
        DocumentFormat::Markdown => crate::adapters::markdown::MarkdownAdapter::build_template(
            source_text,
            rewrite_headings,
        ),
        DocumentFormat::Tex => {
            crate::adapters::tex::TexAdapter::build_template(source_text, rewrite_headings)
        }
    };

    textual_template::slots::build_slots(&template).slots
}

fn normalize_selection_updates(
    slots: &[WritebackSlot],
    response: RewriteUnitResponse,
) -> Result<Vec<SlotUpdate>, String> {
    let mut updates = Vec::with_capacity(response.updates.len());
    for update in response.updates {
        let source_slot = slots
            .iter()
            .find(|slot| slot.id == update.slot_id)
            .ok_or_else(|| format!("未知 slot_id：{}。", update.slot_id))?;
        let normalized = finalize_plain_candidate(&source_slot.text, &update.text)?;
        updates.push(SlotUpdate::new(&update.slot_id, &normalized));
    }
    Ok(updates)
}
