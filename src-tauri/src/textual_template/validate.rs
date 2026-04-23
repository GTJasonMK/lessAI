use crate::rewrite_unit::WritebackSlot;

use super::{
    models::TextTemplate,
    signature::{compute_slot_structure_signature, compute_template_signature},
};

pub(crate) fn ensure_signature_matches(
    expected: Option<&str>,
    actual: &str,
    missing_message: &str,
    mismatch_message: &str,
) -> Result<(), String> {
    let expected = expected.ok_or_else(|| missing_message.to_string())?;
    if actual == expected {
        return Ok(());
    }
    Err(mismatch_message.to_string())
}

pub(crate) fn ensure_template_signature(
    expected: Option<&str>,
    template: &TextTemplate,
) -> Result<(), String> {
    let actual = compute_template_signature(&template.kind, &template.blocks);
    ensure_signature_matches(
        expected,
        &actual,
        "当前会话缺少模板签名，无法校验结构一致性。",
        "当前模板结构与会话记录不一致，无法安全继续。",
    )
}

pub(crate) fn ensure_slot_structure_signature(
    expected: Option<&str>,
    slots: &[WritebackSlot],
) -> Result<(), String> {
    let actual = compute_slot_structure_signature(slots);
    ensure_signature_matches(
        expected,
        &actual,
        "当前会话缺少槽位结构签名，无法校验写回边界。",
        "当前槽位结构与会话记录不一致，无法安全继续。",
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn template_signature_validation_accepts_matching_template() {
        let template = crate::textual_template::models::TextTemplate::single_paragraph(
            "plain_text",
            "txt:p0",
            "第一段\n\n",
        );

        let result =
            super::ensure_template_signature(Some(&template.template_signature), &template);

        assert!(result.is_ok());
    }

    #[test]
    fn slot_structure_signature_validation_accepts_matching_slots() {
        let template = crate::textual_template::models::TextTemplate::single_paragraph(
            "plain_text",
            "txt:p0",
            "第一段\n\n",
        );
        let built = crate::textual_template::slots::build_slots(&template);

        let result = super::ensure_slot_structure_signature(
            Some(&built.slot_structure_signature),
            &built.slots,
        );

        assert!(result.is_ok());
    }
}
