mod capabilities;
mod source;
#[cfg(test)]
mod test_support;
mod textual;
mod writeback;

pub(crate) use capabilities::{
    apply_capability_policy, capability_gate, ensure_capability_allowed,
    hydrate_session_capabilities, hydrated_session_clone, DocumentCapabilityPolicy,
};
pub(crate) use source::{document_session_id, load_document_source, LoadedDocumentSource};
#[cfg(test)]
pub(crate) use test_support::writeback_slots_from_regions;
pub(crate) use textual::document_format;
#[cfg(test)]
pub(crate) use writeback::DocumentWriteback;
pub(crate) use writeback::{
    ensure_document_can_ai_rewrite, ensure_document_can_ai_rewrite_safely,
    ensure_document_source_matches_session, execute_document_writeback,
    normalize_text_against_source_layout, DocumentWritebackContext, OwnedDocumentWriteback,
    WritebackMode,
};

#[cfg(test)]
mod roundtrip_tests;
#[cfg(test)]
mod source_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod writeback_tests;
