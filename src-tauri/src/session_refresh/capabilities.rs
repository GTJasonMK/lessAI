use crate::{
    documents::{apply_capability_policy, capability_gate, DocumentCapabilityPolicy, LoadedDocumentSource},
    models::DocumentSession,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SessionCapabilities {
    policy: DocumentCapabilityPolicy,
}

impl SessionCapabilities {
    pub(super) fn from_loaded(loaded: &LoadedDocumentSource) -> Self {
        Self {
            policy: loaded.capability_policy.clone(),
        }
    }

    pub(super) fn blocked(reason: &str) -> Self {
        Self {
            policy: DocumentCapabilityPolicy::new(
                capability_gate(false, Some(reason)),
                capability_gate(false, Some(reason)),
            ),
        }
    }
}

pub(super) fn apply_session_capabilities(
    session: &mut DocumentSession,
    capabilities: &SessionCapabilities,
) -> bool {
    apply_capability_policy(session, &capabilities.policy)
}

#[cfg(test)]
mod tests {
    use super::{apply_session_capabilities, SessionCapabilities};
    use crate::session_refresh::test_support::sample_session;

    #[test]
    fn apply_session_capabilities_updates_all_capability_fields() {
        let mut session = sample_session();

        let changed = apply_session_capabilities(
            &mut session,
            &SessionCapabilities::blocked("write blocked"),
        );

        assert!(changed);
        assert!(!session.capabilities.source_writeback.allowed);
        assert_eq!(
            session.capabilities.source_writeback.block_reason.as_deref(),
            Some("write blocked")
        );
        assert!(!session.capabilities.editor_writeback.allowed);
        assert_eq!(
            session.capabilities.editor_writeback.block_reason.as_deref(),
            Some("write blocked")
        );
    }

    #[test]
    fn apply_session_capabilities_skips_unchanged_capabilities() {
        let mut session = sample_session();
        let loaded = crate::documents::LoadedDocumentSource {
            source_text: session.source_text.clone(),
            template_kind: session.template_kind.clone(),
            template_signature: session.template_signature.clone(),
            slot_structure_signature: session.slot_structure_signature.clone(),
            template_snapshot: session.template_snapshot.clone(),
            writeback_slots: session.writeback_slots.clone(),
            capability_policy: crate::documents::DocumentCapabilityPolicy::new(
                crate::documents::capability_gate(
                    session.capabilities.source_writeback.allowed,
                    session.capabilities.source_writeback.block_reason.as_deref(),
                ),
                crate::documents::capability_gate(
                    session.capabilities.editor_writeback.allowed,
                    session.capabilities.editor_writeback.block_reason.as_deref(),
                ),
            ),
        };

        let changed = apply_session_capabilities(
            &mut session,
            &SessionCapabilities::from_loaded(&loaded),
        );

        assert!(!changed);
    }
}
