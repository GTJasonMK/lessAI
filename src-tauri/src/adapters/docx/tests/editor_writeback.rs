use super::*;

#[test]
fn writes_back_updated_text_for_simple_docx() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:pPr><w:pStyle w:val="Heading1"/></w:pPr>
      <w:r><w:t>标题</w:t></w:r>
    </w:p>
    <w:p><w:r><w:t>正文</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let source = DocxAdapter::extract_text(&bytes).expect("extract text");
    let updated = "标题\n\n改写后的正文";

    let rewritten =
        DocxAdapter::write_updated_text(&bytes, &source, updated).expect("write updated text");
    let extracted = DocxAdapter::extract_text(&rewritten).expect("extract updated text");

    assert_eq!(extracted, updated);
}

#[test]
fn validates_editor_writeback_for_docx_with_single_styled_region_per_paragraph() {
    let bytes = build_rfonts_hint_fragmented_docx();

    DocxAdapter::validate_editor_writeback(&bytes).expect("plain text editor should be allowed");
}

#[test]
fn validates_editor_writeback_for_docx_with_multiple_editable_regions() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:rPr><w:b/></w:rPr><w:t>加粗</w:t></w:r>
      <w:r><w:t>正文</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);

    DocxAdapter::validate_editor_writeback(&bytes).expect("plain text editor should be allowed");
}

#[test]
fn writes_back_updated_text_for_docx_with_single_styled_region_per_paragraph() {
    let bytes = build_rfonts_hint_fragmented_docx();
    let source = DocxAdapter::extract_text(&bytes).expect("extract text");
    let updated = "2026年（第19届）";

    let rewritten =
        DocxAdapter::write_updated_text(&bytes, &source, updated).expect("write updated text");
    let extracted = DocxAdapter::extract_text(&rewritten).expect("extract updated text");
    let document_xml = read_docx_entry(&rewritten, "word/document.xml");

    assert_eq!(extracted, updated);
    assert!(document_xml.contains("<w:b/>"));
    assert!(document_xml.contains("华文中宋"));
}

#[test]
fn writes_back_updated_text_for_docx_with_multiple_editable_regions_when_edit_stays_in_region() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:rPr><w:b/></w:rPr><w:t>加粗</w:t></w:r>
      <w:r><w:t>正文</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let source = DocxAdapter::extract_text(&bytes).expect("extract text");
    let updated = "加粗正新文";

    let rewritten =
        DocxAdapter::write_updated_text(&bytes, &source, updated).expect("write updated text");
    let extracted = DocxAdapter::extract_text(&rewritten).expect("extract updated text");
    let document_xml = read_docx_entry(&rewritten, "word/document.xml");

    assert_eq!(extracted, updated);
    assert!(document_xml.contains("<w:b/>"));
}

#[test]
fn writes_back_docx_with_paragraph_level_stray_text_nodes_as_locked_visible_regions() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>前<w:r><w:t>正文</w:t></w:r>后</w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let source =
        DocxAdapter::extract_writeback_source_text(&bytes).expect("extract writeback source");
    let regions =
        DocxAdapter::extract_writeback_regions(&bytes).expect("extract writeback regions");

    assert_eq!(source, "前正文后");
    assert!(regions
        .iter()
        .any(|region| region.skip_rewrite && region.body.contains("前")));
    assert_region_with_text_editable(&regions, "正文");
    assert!(regions
        .iter()
        .any(|region| region.skip_rewrite && region.body.contains("后")));

    let rewritten =
        DocxAdapter::write_updated_text(&bytes, &source, &source).expect("write updated text");
    let rewritten_source = DocxAdapter::extract_writeback_source_text(&rewritten)
        .expect("extract rewritten writeback source");

    assert_eq!(rewritten_source, source);
}

#[test]
fn writes_back_docx_with_run_level_stray_text_nodes_as_locked_visible_regions() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:rPr><w:b/></w:rPr>前<w:t>正文</w:t>后</w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let source =
        DocxAdapter::extract_writeback_source_text(&bytes).expect("extract writeback source");
    let regions =
        DocxAdapter::extract_writeback_regions(&bytes).expect("extract writeback regions");

    assert_eq!(source, "前正文后");
    assert!(regions
        .iter()
        .any(|region| region.skip_rewrite && region.body.contains("前")));
    assert_region_with_text_editable(&regions, "正文");
    assert!(regions
        .iter()
        .any(|region| region.skip_rewrite && region.body.contains("后")));

    let rewritten =
        DocxAdapter::write_updated_text(&bytes, &source, &source).expect("write updated text");
    let rewritten_source = DocxAdapter::extract_writeback_source_text(&rewritten)
        .expect("extract rewritten writeback source");
    let rewritten_document_xml = read_docx_entry(&rewritten, "word/document.xml");

    assert_eq!(rewritten_source, source);
    assert!(rewritten_document_xml.contains("<w:b/>"));
}

#[test]
fn writes_back_updated_text_for_docx_when_edit_crosses_style_boundary() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:rPr><w:b/></w:rPr><w:t>加粗</w:t></w:r>
      <w:r><w:t>正文</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let source = DocxAdapter::extract_text(&bytes).expect("extract text");
    let updated = "加X文";

    let rewritten =
        DocxAdapter::write_updated_text(&bytes, &source, updated).expect("write updated text");
    let extracted = DocxAdapter::extract_text(&rewritten).expect("extract updated text");
    let document_xml = read_docx_entry(&rewritten, "word/document.xml");

    assert_eq!(extracted, updated);
    assert!(document_xml.contains("<w:b/>"));
}

#[test]
fn writes_back_updated_text_for_docx_when_inserting_at_style_boundary() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:rPr><w:b/></w:rPr><w:t>甲</w:t></w:r>
      <w:r><w:t>乙</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let source = DocxAdapter::extract_text(&bytes).expect("extract text");
    let updated = "甲X乙";

    let rewritten =
        DocxAdapter::write_updated_text(&bytes, &source, updated).expect("write updated text");
    let extracted = DocxAdapter::extract_text(&rewritten).expect("extract updated text");
    let document_xml = read_docx_entry(&rewritten, "word/document.xml");

    assert_eq!(extracted, updated);
    assert!(document_xml.contains("<w:b/>"));
}

#[test]
fn validates_editor_writeback_for_report_template() {
    let bytes = load_repo_docx_fixture("04-3 作品报告（大数据应用赛，2025版）模板.docx");

    DocxAdapter::validate_editor_writeback(&bytes).expect("plain text editor should be allowed");
}

#[test]
fn validates_editor_writeback_for_docx_with_inline_locked_formula() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document
  xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
  xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math">
  <w:body>
    <w:p>
      <w:r><w:t>前文</w:t></w:r>
      <m:oMath><m:r><m:t>E=mc^2</m:t></m:r></m:oMath>
      <w:r><w:t>后文</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);

    DocxAdapter::validate_editor_writeback(&bytes).expect("plain text editor should be allowed");
}

#[test]
fn writes_back_updated_text_for_docx_with_inline_locked_formula() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document
  xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
  xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math">
  <w:body>
    <w:p>
      <w:r><w:t>前文</w:t></w:r>
      <m:oMath><m:r><m:t>E=mc^2</m:t></m:r></m:oMath>
      <w:r><w:t>后文</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let source = DocxAdapter::extract_text(&bytes).expect("extract text");
    let updated = "新前文E=mc^2新后文";

    let rewritten =
        DocxAdapter::write_updated_text(&bytes, &source, updated).expect("write updated text");
    let extracted = DocxAdapter::extract_text(&rewritten).expect("extract updated text");
    let document_xml = read_docx_entry(&rewritten, "word/document.xml");

    assert_eq!(extracted, updated);
    assert!(document_xml.contains("<m:oMath>"));
}

#[test]
fn writes_back_updated_text_for_docx_with_paragraph_level_drawing() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
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
    let bytes = build_minimal_docx(xml);
    let source = DocxAdapter::extract_writeback_source_text(&bytes).expect("extract source");
    let updated = "新前文[图表]新后文";

    let rewritten =
        DocxAdapter::write_updated_text(&bytes, &source, updated).expect("write updated text");
    let extracted = DocxAdapter::extract_text(&rewritten).expect("extract updated text");
    let document_xml = read_docx_entry(&rewritten, "word/document.xml");

    assert_eq!(source, "前文[图表]后文");
    assert_eq!(extracted, updated);
    assert!(document_xml.contains("<w:drawing>"));
}

#[test]
fn rejects_full_text_writeback_when_edit_crosses_locked_formula_boundary() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document
  xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
  xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math">
  <w:body>
    <w:p>
      <w:r><w:t>前文</w:t></w:r>
      <m:oMath><m:r><m:t>E=mc^2</m:t></m:r></m:oMath>
      <w:r><w:t>后文</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let source = DocxAdapter::extract_text(&bytes).expect("extract text");

    let error = DocxAdapter::write_updated_text(&bytes, &source, "新前文E=mc^3新后文")
        .expect_err("crossing locked formula boundary should be rejected");

    assert!(error.contains("边界") || error.contains("锁定"));
}

#[test]
fn writes_back_updated_text_for_simple_docx_with_tabs() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:t>甲</w:t></w:r>
      <w:r><w:tab/></w:r>
      <w:r><w:t>乙</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let source = DocxAdapter::extract_text(&bytes).expect("extract text");
    let updated = "丙\t丁";

    let rewritten =
        DocxAdapter::write_updated_text(&bytes, &source, updated).expect("write updated text");
    let extracted = DocxAdapter::extract_text(&rewritten).expect("extract updated text");

    assert_eq!(extracted, updated);
}

#[test]
fn writes_back_updated_text_for_simple_docx_with_line_breaks() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:t>甲</w:t></w:r>
      <w:r><w:br/></w:r>
      <w:r><w:t>乙</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let source = DocxAdapter::extract_text(&bytes).expect("extract text");
    let updated = "丙\n丁";

    let rewritten =
        DocxAdapter::write_updated_text(&bytes, &source, updated).expect("write updated text");
    let extracted = DocxAdapter::extract_text(&rewritten).expect("extract updated text");

    assert_eq!(extracted, updated);
}

#[test]
fn writes_back_updated_slots_for_docx_with_manual_line_breaks() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:t>甲</w:t><w:br/><w:t>乙</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let source = DocxAdapter::extract_writeback_source_text(&bytes).expect("extract source");
    let mut slots = DocxAdapter::extract_writeback_slots(&bytes, false).expect("extract slots");

    slots[0].text = "丙".to_string();
    slots[1].text = "丁".to_string();

    let rewritten =
        DocxAdapter::write_updated_slots(&bytes, &source, &slots).expect("write updated slots");
    let extracted = DocxAdapter::extract_text(&rewritten).expect("extract updated text");

    assert_eq!(extracted, "丙\n丁");
}

#[test]
fn writes_back_updated_text_for_simple_docx_with_list_paragraphs() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:pPr>
        <w:numPr>
          <w:ilvl w:val="0"/>
          <w:numId w:val="1"/>
        </w:numPr>
      </w:pPr>
      <w:r><w:t>第一项</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let source = DocxAdapter::extract_text(&bytes).expect("extract text");
    let updated = "改写后的列表项";

    let rewritten =
        DocxAdapter::write_updated_text(&bytes, &source, updated).expect("write updated text");
    let extracted = DocxAdapter::extract_text(&rewritten).expect("extract updated text");
    let document_xml = read_docx_entry(&rewritten, "word/document.xml");

    assert_eq!(extracted, updated);
    assert!(document_xml.contains("<w:numPr>"));
    assert!(document_xml.contains("<w:numId"));
}
