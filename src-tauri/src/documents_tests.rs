use std::fs;

use super::{
    ensure_document_source_matches_session, execute_document_writeback, load_document_source,
    normalize_text_against_source_layout, DocumentWriteback, DocumentWritebackContext,
    WritebackMode,
};
use crate::document_snapshot::{capture_document_snapshot, SNAPSHOT_MISSING_ERROR};
use crate::models::SegmentationPreset;
use crate::rewrite_unit::build_rewrite_units;
use crate::test_support::{
    build_docx_entries, build_minimal_docx, build_minimal_pdf, cleanup_dir, write_temp_file,
};

fn rebuild_source_text(loaded: &super::LoadedDocumentSource) -> String {
    loaded
        .writeback_slots
        .iter()
        .map(|slot| format!("{}{}", slot.text, slot.separator_after))
        .collect::<String>()
}

#[test]
fn decode_utf8_bom_text_file() {
    let bytes = [0xEF, 0xBB, 0xBF, b'a', b'b', b'c'];
    assert_eq!(super::textual::decode_text_file(&bytes).unwrap(), "abc");
}

#[test]
fn decode_utf16_le_bom_text_file() {
    let bytes = [0xFF, 0xFE, b'A', 0x00, b'\n', 0x00];
    assert_eq!(super::textual::decode_text_file(&bytes).unwrap(), "A\n");
}

#[test]
fn decode_utf16_be_bom_text_file() {
    let bytes = [0xFE, 0xFF, 0x00, b'A', 0x00, b'\n'];
    assert_eq!(super::textual::decode_text_file(&bytes).unwrap(), "A\n");
}

#[test]
fn decode_invalid_text_file_returns_error() {
    let bytes = [0xFF, 0xFF, 0xFF];
    assert!(super::textual::decode_text_file(&bytes).is_err());
}

#[test]
fn document_format_maps_docx_extension_to_docx() {
    assert_eq!(
        super::textual::document_format(std::path::Path::new("/tmp/demo.docx")),
        crate::models::DocumentFormat::Docx
    );
}

#[test]
fn document_format_maps_pdf_extension_to_pdf() {
    assert_eq!(
        super::textual::document_format(std::path::Path::new("/tmp/demo.pdf")),
        crate::models::DocumentFormat::Pdf
    );
}

#[test]
fn load_pdf_source_returns_pdf_template_metadata() {
    let bytes = build_minimal_pdf(&["Alpha line", "Beta line"]);
    let (root, target) = write_temp_file("pdf-source", "pdf", &bytes);

    let loaded = load_document_source(&target, false).expect("load pdf");

    assert_eq!(loaded.template_kind.as_deref(), Some("pdf"));
    assert_eq!(loaded.source_text, "Alpha line\nBeta line\n");
    assert_eq!(rebuild_source_text(&loaded), loaded.source_text);
    assert!(loaded.template_signature.is_some());
    assert!(loaded.slot_structure_signature.is_some());

    cleanup_dir(&root);
}

#[test]
fn docx_without_writeback_support_is_not_allowed_to_continue_ai_rewrite() {
    let error = super::writeback::ensure_document_can_ai_rewrite(
        &crate::session_capability_models::CapabilityGate::blocked(
            "当前 docx 暂不支持安全写回覆盖。",
        ),
    )
    .expect_err("expected rewrite guard");

    assert!(error.contains("docx") || error.contains("写回"));
}

#[test]
fn pdf_without_writeback_support_is_not_allowed_to_continue_ai_rewrite() {
    let error = super::writeback::ensure_document_can_ai_rewrite(
        &crate::session_capability_models::CapabilityGate::blocked(
            "当前 PDF 的文本层结构不足以安全写回原文件。",
        ),
    )
    .expect_err("expected rewrite guard");

    assert!(error.contains("PDF") || error.contains("写回"));
}

#[test]
fn normalize_text_against_source_layout_reuses_plain_text_layout_rules() {
    let normalized =
        normalize_text_against_source_layout("原文  \r\n下一行\r\n", "新文  \n下一行  \n");

    assert_eq!(normalized, "新文  \r\n下一行  \r\n");
}

#[test]
fn write_document_content_rejects_external_change_for_plain_text() {
    let (root, target) = write_temp_file("plain-writeback-mismatch", "txt", "原始内容".as_bytes());
    let snapshot = capture_document_snapshot(&target).expect("capture snapshot");

    fs::write(&target, "外部修改").expect("simulate external change");

    let error = execute_document_writeback(
        &target,
        DocumentWritebackContext::new("原始内容", Some(&snapshot)),
        DocumentWriteback::Text("新的内容"),
        WritebackMode::Write,
    )
    .expect_err("expected mismatch error");
    assert!(error.contains("原文件已在外部发生变化"));

    cleanup_dir(&root);
}

#[test]
fn ensure_document_source_matches_session_rejects_external_change_for_plain_text() {
    let (root, target) =
        write_temp_file("plain-source-guard-mismatch", "txt", "原始内容".as_bytes());
    let snapshot = capture_document_snapshot(&target).expect("capture snapshot");

    fs::write(&target, "外部修改").expect("simulate external change");

    let error = ensure_document_source_matches_session(&target, Some(&snapshot))
        .expect_err("expected mismatch error");
    assert!(error.contains("原文件已在外部发生变化"));

    cleanup_dir(&root);
}

#[test]
fn write_document_content_rejects_plain_text_without_snapshot_even_when_source_matches() {
    let (root, target) = write_temp_file(
        "plain-writeback-without-snapshot",
        "txt",
        "原始内容".as_bytes(),
    );

    let error = execute_document_writeback(
        &target,
        DocumentWritebackContext::new("原始内容", None),
        DocumentWriteback::Text("新的内容"),
        WritebackMode::Write,
    )
    .expect_err("expected missing snapshot to be rejected");

    assert_eq!(error, SNAPSHOT_MISSING_ERROR);

    cleanup_dir(&root);
}

#[test]
fn write_document_content_rejects_docx_without_snapshot_even_when_source_matches() {
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>原文</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(document_xml);
    let (root, target) = write_temp_file("docx-writeback-without-snapshot", "docx", &bytes);

    let error = execute_document_writeback(
        &target,
        DocumentWritebackContext::new("原文", None),
        DocumentWriteback::Text("新正文"),
        WritebackMode::Write,
    )
    .expect_err("expected missing snapshot to be rejected");

    assert_eq!(error, SNAPSHOT_MISSING_ERROR);

    cleanup_dir(&root);
}

#[test]
fn load_markdown_source_returns_writeback_slots() {
    let markdown =
        "# 标题\n正文里的 `code` 和 [链接](https://example.com)。\n\n```ts\nconst x = 1;\n```\n";
    let (root, target) = write_temp_file("markdown-source", "md", markdown.as_bytes());

    let loaded = load_document_source(&target, false).expect("load markdown");

    assert_eq!(loaded.source_text, markdown);
    assert_eq!(rebuild_source_text(&loaded), markdown);
    assert!(loaded.writeback_slots.iter().any(|slot| !slot.editable));
    assert!(loaded
        .writeback_slots
        .iter()
        .any(|slot| slot.text.contains("`code`")));
    assert!(loaded
        .writeback_slots
        .iter()
        .any(|slot| slot.text.contains("```ts")));

    cleanup_dir(&root);
}

#[test]
fn load_markdown_source_protects_mixed_inline_structures_without_locking_prose() {
    let markdown = "这一整段故意写得比较长，用来观察 Markdown 正文在复杂混排场景下的稳定性：它同时包含中文说明、English phrases、数字 0.618、版本号 `v3.1.4`、引用式链接 [深入说明][docs]、行内代码 `cargo test docx -- --nocapture`、公式 $f(x)=x^2$、引用标记 [@ref-demo]、脚注引用[^note1]、以及一个裸地址 https://example.com/report/final；段落模式应优先保证整体可读，整句模式应主要在真正句末切分，小句模式则可以更细，但任何模式都不应吞掉 Markdown 语法或把一个小结构拆成一串不可理解的碎片。\n";
    let (root, target) = write_temp_file("markdown-inline-guards", "md", markdown.as_bytes());

    let loaded = load_document_source(&target, false).expect("load markdown");

    assert_eq!(loaded.source_text, markdown);
    assert_eq!(rebuild_source_text(&loaded), markdown);

    for protected in [
        "`v3.1.4`",
        "[深入说明][docs]",
        "`cargo test docx -- --nocapture`",
        "$f(x)=x^2$",
        "[@ref-demo]",
        "[^note1]",
        "https://example.com/report/final",
    ] {
        let slot = loaded
            .writeback_slots
            .iter()
            .find(|slot| slot.text == protected)
            .unwrap_or_else(|| panic!("missing protected slot: {protected}"));
        assert!(
            !slot.editable,
            "expected protected slot to stay locked: {protected}"
        );
    }

    for editable in [
        "这一整段故意写得比较长",
        "English phrases",
        "段落模式应优先保证整体可读",
        "任何模式都不应吞掉 Markdown 语法",
    ] {
        let slot = loaded
            .writeback_slots
            .iter()
            .find(|slot| slot.text.contains(editable))
            .unwrap_or_else(|| panic!("missing editable slot: {editable}"));
        assert!(
            slot.editable,
            "expected prose slot to stay editable: {editable}"
        );
    }

    cleanup_dir(&root);
}

#[test]
fn load_tex_source_returns_writeback_slots() {
    let tex = "\\section{标题}\n正文和公式 $x+y$。\n% 注释\n";
    let (root, target) = write_temp_file("tex-source", "tex", tex.as_bytes());

    let loaded = load_document_source(&target, false).expect("load tex");

    assert_eq!(loaded.source_text, tex);
    assert_eq!(rebuild_source_text(&loaded), tex);
    assert!(loaded.writeback_slots.iter().any(|slot| !slot.editable));
    assert!(loaded
        .writeback_slots
        .iter()
        .any(|slot| slot.text.contains("\\section")));
    assert!(loaded
        .writeback_slots
        .iter()
        .any(|slot| slot.text.contains("$x+y$")));

    cleanup_dir(&root);
}

#[test]
fn load_plain_text_source_returns_atomic_editable_slots() {
    let text = "第一句。\n第二句。";
    let (root, target) = write_temp_file("plain-source", "txt", text.as_bytes());

    let loaded = load_document_source(&target, false).expect("load text");

    assert_eq!(loaded.source_text, text);
    assert_eq!(rebuild_source_text(&loaded), text);
    assert_eq!(loaded.writeback_slots.len(), 2);
    assert_eq!(loaded.writeback_slots[0].text, "第一句。");
    assert_eq!(loaded.writeback_slots[0].separator_after, "\n");
    assert_eq!(loaded.writeback_slots[1].text, "第二句。");
    assert!(loaded.writeback_slots.iter().all(|slot| slot.editable));

    cleanup_dir(&root);
}

#[test]
fn load_tex_source_uses_atomic_slots_so_segmentation_preset_changes_unit_count() {
    let tex = "\\section{标题}\n\n第一句。第二句，第三句。";
    let (root, target) = write_temp_file("tex-segmentation", "tex", tex.as_bytes());

    let loaded = load_document_source(&target, false).expect("load tex");

    let paragraph_units =
        build_rewrite_units(&loaded.writeback_slots, SegmentationPreset::Paragraph);
    let sentence_units = build_rewrite_units(&loaded.writeback_slots, SegmentationPreset::Sentence);
    let clause_units = build_rewrite_units(&loaded.writeback_slots, SegmentationPreset::Clause);

    assert_eq!(paragraph_units.len(), 2);
    assert_eq!(sentence_units.len(), 3);
    assert_eq!(clause_units.len(), 4);
    assert_eq!(sentence_units[1].display_text, "第一句。");
    assert_eq!(sentence_units[2].display_text, "第二句，第三句。");
    assert_eq!(clause_units[1].display_text, "第一句。");
    assert_eq!(clause_units[2].display_text, "第二句，");
    assert_eq!(clause_units[3].display_text, "第三句。");

    cleanup_dir(&root);
}

#[test]
fn load_docx_source_preserves_writeback_slot_boundaries() {
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:pPr><w:pStyle w:val="CustomHeading"/></w:pPr>
      <w:r><w:t>标题</w:t></w:r>
    </w:p>
    <w:p><w:r><w:t>正文</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
    let styles_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:style w:type="paragraph" w:styleId="CustomHeading">
    <w:pPr><w:outlineLvl w:val="0"/></w:pPr>
  </w:style>
</w:styles>"#;
    let bytes = build_docx_entries(&[
        ("word/document.xml", document_xml),
        ("word/styles.xml", styles_xml),
    ]);
    let (root, target) = write_temp_file("docx-source", "docx", &bytes);

    let loaded = load_document_source(&target, false).expect("load docx");

    assert_eq!(loaded.template_kind.as_deref(), Some("docx"));
    assert_eq!(rebuild_source_text(&loaded), loaded.source_text);
    assert!(loaded.writeback_slots.iter().any(|slot| !slot.editable));
    assert!(loaded
        .writeback_slots
        .iter()
        .any(|slot| slot.text.contains("标题")));
    assert!(loaded
        .writeback_slots
        .iter()
        .any(|slot| slot.text.contains("正文")));
    assert!(loaded.template_signature.is_some());
    assert!(loaded.slot_structure_signature.is_some());

    cleanup_dir(&root);
}
