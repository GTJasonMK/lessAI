use crate::{
    adapters::{markdown::MarkdownAdapter, tex::TexAdapter, TextRegion},
    documents::writeback_slots_from_regions,
    models::{AppSettings, DocumentFormat},
    rewrite_unit::{
        apply_slot_updates, build_rewrite_unit_request_from_slots, merged_text_from_slots,
        RewriteUnitResponse, SlotUpdate, WritebackSlot,
    },
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
    if !slots.iter().any(|slot| slot.editable && !slot.text.trim().is_empty()) {
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
    let regions = match format {
        DocumentFormat::PlainText => vec![TextRegion {
            body: source_text.to_string(),
            skip_rewrite: false,
            presentation: None,
        }],
        DocumentFormat::Markdown => MarkdownAdapter::split_regions(source_text, rewrite_headings),
        DocumentFormat::Tex => TexAdapter::split_regions(source_text, rewrite_headings),
    };
    writeback_slots_from_regions(&regions)
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
