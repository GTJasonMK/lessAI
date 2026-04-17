pub(crate) mod build;
pub(crate) mod models;
pub(crate) mod projection;
pub(crate) mod protocol;
pub(crate) mod session;

pub(crate) use build::build_rewrite_units;
pub(crate) use projection::{apply_slot_updates, merged_text_from_slots};
pub(crate) use models::{
    RewriteSuggestion, RewriteUnit, SlotUpdate, WritebackSlot, WritebackSlotRole,
};
pub(crate) use protocol::{
    parse_rewrite_batch_response, parse_rewrite_unit_response, RewriteBatchRequest,
    RewriteBatchResponse, RewriteUnitRequest, RewriteUnitResponse, RewriteUnitSlot,
};
pub(crate) use session::{
    build_rewrite_unit_request, build_rewrite_unit_request_from_slots, find_rewrite_unit, rewrite_unit_text,
    rewrite_unit_text_with_updates,
};
