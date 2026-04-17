use super::{execute_document_writeback, load_document_source, DocumentWriteback, WritebackMode};
use crate::test_support::{build_minimal_docx, cleanup_dir, write_temp_file};
use crate::{adapters::docx::DocxAdapter, document_snapshot::capture_document_snapshot};

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
        "标题正文",
        Some(&snapshot),
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
        "前文[图表]后文",
        Some(&snapshot),
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
        "第一段\n\n第二段",
        Some(&snapshot),
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
        "第一段\n\n第二段",
        Some(&snapshot),
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
    let snapshot = capture_document_snapshot(&target).expect("capture snapshot");
    let mut slots = DocxAdapter::extract_writeback_slots(&bytes, false).expect("extract slots");
    slots[0].text = "新前文".to_string();
    slots[1].text = "新后文".to_string();

    execute_document_writeback(
        &target,
        "前文后文",
        Some(&snapshot),
        DocumentWriteback::Slots(&slots),
        WritebackMode::Validate,
    )
    .expect("expected region validation to preserve real adapter metadata");

    cleanup_dir(&root);
}
