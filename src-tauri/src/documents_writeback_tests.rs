use super::{
    execute_document_writeback, load_document_source, DocumentWriteback, DocumentWritebackContext,
    WritebackMode,
};
use crate::test_support::{build_minimal_docx, cleanup_dir, write_temp_file};
use crate::document_snapshot::capture_document_snapshot;

fn textual_writeback_context<'a>(
    loaded: &'a super::LoadedDocumentSource,
    snapshot: &'a crate::models::DocumentSnapshot,
) -> DocumentWritebackContext<'a> {
    DocumentWritebackContext::new(&loaded.source_text, Some(snapshot)).with_structure_signatures(
        loaded.template_signature.as_deref(),
        loaded.slot_structure_signature.as_deref(),
        false,
    )
}

#[test]
fn write_document_content_allows_docx_when_styled_prefix_becomes_empty() {
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:rPr><w:b/></w:rPr><w:t>标题</w:t></w:r>
      <w:r><w:t>正文</w:t></w:r>
    </w:p>
  </w:body>
    </w:document>"#;
    let bytes = build_minimal_docx(document_xml);
    let (root, target) = write_temp_file("docx-empty-styled-prefix", "docx", &bytes);
    let snapshot = capture_document_snapshot(&target).expect("capture snapshot");

    execute_document_writeback(
        &target,
        DocumentWritebackContext::new("标题正文", Some(&snapshot)),
        DocumentWriteback::Text("正文"),
        WritebackMode::Write,
    )
    .expect("docx write should preserve empty styled boundary safely");

    let loaded = load_document_source(&target, false).expect("reload docx");
    assert_eq!(loaded.source_text, "正文");

    cleanup_dir(&root);
}

#[test]
fn write_document_content_allows_docx_with_paragraph_level_drawing_placeholder() {
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
            xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
            xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
            xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
            xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart">
  <w:body>
    <w:p>
      <w:r><w:t>前文</w:t></w:r>
      <w:drawing>
        <wp:inline>
          <a:graphic>
            <a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart">
              <c:chart r:id="rIdChart1"/>
            </a:graphicData>
          </a:graphic>
        </wp:inline>
      </w:drawing>
      <w:r><w:t>后文</w:t></w:r>
    </w:p>
  </w:body>
    </w:document>"#;
    let bytes = build_minimal_docx(document_xml);
    let (root, target) = write_temp_file("docx-paragraph-level-drawing", "docx", &bytes);
    let snapshot = capture_document_snapshot(&target).expect("capture snapshot");

    execute_document_writeback(
        &target,
        DocumentWritebackContext::new("前文[图表]后文", Some(&snapshot)),
        DocumentWriteback::Text("新前文[图表]新后文"),
        WritebackMode::Write,
    )
    .expect("docx write should preserve paragraph-level drawing placeholder safely");

    let loaded = load_document_source(&target, false).expect("reload docx");
    assert_eq!(loaded.source_text, "新前文[图表]新后文");

    cleanup_dir(&root);
}

#[test]
fn validate_document_writeback_rejects_docx_when_paragraph_count_changes() {
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>第一段</w:t></w:r></w:p>
    <w:p><w:r><w:t>第二段</w:t></w:r></w:p>
  </w:body>
    </w:document>"#;
    let bytes = build_minimal_docx(document_xml);
    let (root, target) = write_temp_file("paragraph-count-fail", "docx", &bytes);
    let snapshot = capture_document_snapshot(&target).expect("capture snapshot");

    let error = execute_document_writeback(
        &target,
        DocumentWritebackContext::new("第一段\n\n第二段", Some(&snapshot)),
        DocumentWriteback::Text("第一段\n\n新增段\n\n第二段"),
        WritebackMode::Validate,
    )
    .expect_err("expected paragraph count validation failure");

    assert!(error.contains("段落数量保持不变") || error.contains("简单 docx"));

    cleanup_dir(&root);
}

#[test]
fn validate_document_writeback_allows_docx_when_structure_stays_compatible() {
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>第一段</w:t></w:r></w:p>
    <w:p><w:r><w:t>第二段</w:t></w:r></w:p>
  </w:body>
    </w:document>"#;
    let bytes = build_minimal_docx(document_xml);
    let (root, target) = write_temp_file("paragraph-count-pass", "docx", &bytes);
    let snapshot = capture_document_snapshot(&target).expect("capture snapshot");

    execute_document_writeback(
        &target,
        DocumentWritebackContext::new("第一段\n\n第二段", Some(&snapshot)),
        DocumentWriteback::Text("改写第一段\n\n改写第二段"),
        WritebackMode::Validate,
    )
    .expect("expected structure-compatible edit to validate");

    cleanup_dir(&root);
}

#[test]
fn validate_document_writeback_allows_docx_regions_with_adjacent_styles() {
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:rPr><w:b/></w:rPr><w:t>前文</w:t></w:r>
      <w:r><w:rPr><w:u w:val="single"/></w:rPr><w:t>后文</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(document_xml);
    let (root, target) = write_temp_file("adjacent-styled-region-pass", "docx", &bytes);
    let loaded = load_document_source(&target, false).expect("load docx");
    let snapshot = capture_document_snapshot(&target).expect("capture snapshot");
    let mut slots = loaded.writeback_slots.clone();
    slots[0].text = "新前文".to_string();
    slots[1].text = "新后文".to_string();

    execute_document_writeback(
        &target,
        textual_writeback_context(&loaded, &snapshot),
        DocumentWriteback::Slots(&slots),
        WritebackMode::Validate,
    )
    .expect("expected region validation to preserve real adapter metadata");

    cleanup_dir(&root);
}

#[test]
fn validate_document_writeback_rejects_plain_text_slot_reorder() {
    let (root, target) =
        write_temp_file("plain-slot-reorder", "txt", "第一段\n\n第二段".as_bytes());
    let loaded = load_document_source(&target, false).expect("load plain text");
    let snapshot = capture_document_snapshot(&target).expect("capture snapshot");
    let mut slots = loaded.writeback_slots.clone();
    slots.swap(0, 1);

    let error = execute_document_writeback(
        &target,
        textual_writeback_context(&loaded, &snapshot),
        DocumentWriteback::Slots(&slots),
        WritebackMode::Validate,
    )
    .expect_err("reordered plain-text slots should fail");

    assert!(error.contains("结构"));
    cleanup_dir(&root);
}

#[test]
fn validate_document_writeback_rejects_markdown_slot_reorder() {
    let (root, target) =
        write_temp_file("markdown-slot-reorder", "md", "第一段\n\n第二段".as_bytes());
    let loaded = load_document_source(&target, false).expect("load markdown");
    let snapshot = capture_document_snapshot(&target).expect("capture snapshot");
    let mut slots = loaded.writeback_slots.clone();
    slots.swap(0, 1);

    let error = execute_document_writeback(
        &target,
        textual_writeback_context(&loaded, &snapshot),
        DocumentWriteback::Slots(&slots),
        WritebackMode::Validate,
    )
    .expect_err("reordered markdown slots should fail");

    assert!(error.contains("结构"));
    cleanup_dir(&root);
}

#[test]
fn validate_document_writeback_rejects_tex_slot_reorder() {
    let (root, target) = write_temp_file("tex-slot-reorder", "tex", "第一段\n\n第二段".as_bytes());
    let loaded = load_document_source(&target, false).expect("load tex");
    let snapshot = capture_document_snapshot(&target).expect("capture snapshot");
    let mut slots = loaded.writeback_slots.clone();
    slots.swap(0, 1);

    let error = execute_document_writeback(
        &target,
        textual_writeback_context(&loaded, &snapshot),
        DocumentWriteback::Slots(&slots),
        WritebackMode::Validate,
    )
    .expect_err("reordered tex slots should fail");

    assert!(error.contains("结构"));
    cleanup_dir(&root);
}

#[test]
fn validate_document_writeback_allows_markdown_slot_text_updates() {
    let (root, target) =
        write_temp_file("markdown-slot-update", "md", "第一段\n\n第二段".as_bytes());
    let loaded = load_document_source(&target, false).expect("load markdown");
    let snapshot = capture_document_snapshot(&target).expect("capture snapshot");
    let mut slots = loaded.writeback_slots.clone();
    let first_editable = slots
        .iter_mut()
        .find(|slot| slot.editable)
        .expect("first editable slot");
    first_editable.text = "改写第一段".to_string();

    execute_document_writeback(
        &target,
        textual_writeback_context(&loaded, &snapshot),
        DocumentWriteback::Slots(&slots),
        WritebackMode::Validate,
    )
    .expect("markdown slot text update should validate");

    cleanup_dir(&root);
}

#[test]
fn validate_document_writeback_allows_tex_slot_text_updates() {
    let (root, target) = write_temp_file("tex-slot-update", "tex", "第一段\n\n第二段".as_bytes());
    let loaded = load_document_source(&target, false).expect("load tex");
    let snapshot = capture_document_snapshot(&target).expect("capture snapshot");
    let mut slots = loaded.writeback_slots.clone();
    let first_editable = slots
        .iter_mut()
        .find(|slot| slot.editable)
        .expect("first editable slot");
    first_editable.text = "改写第一段".to_string();

    execute_document_writeback(
        &target,
        textual_writeback_context(&loaded, &snapshot),
        DocumentWriteback::Slots(&slots),
        WritebackMode::Validate,
    )
    .expect("tex slot text update should validate");

    cleanup_dir(&root);
}

#[test]
fn validate_document_writeback_allows_pdf_text_projection_without_source_reload() {
    let (root, target) = write_temp_file("pdf-validate", "pdf", b"%PDF-1.4\n");

    execute_document_writeback(
        &target,
        DocumentWritebackContext::new("原文", None),
        DocumentWriteback::Text("改写后"),
        WritebackMode::Validate,
    )
    .expect("pdf validate should allow export-style text projection");

    cleanup_dir(&root);
}
