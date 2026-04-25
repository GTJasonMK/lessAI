use super::*;

#[test]
fn extracts_tables_as_locked_placeholders() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:tbl>
      <w:tr>
        <w:tc>
          <w:p><w:r><w:t>表格内容</w:t></w:r></w:p>
        </w:tc>
      </w:tr>
    </w:tbl>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);

    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");
    assert_eq!(regions.len(), 1);
    assert!(regions[0].skip_rewrite);
    assert_eq!(regions[0].body, "[表格]");
}

#[test]
fn extracts_numbered_lists_as_regions() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:pPr><w:numPr><w:ilvl w:val="0"/></w:numPr></w:pPr>
      <w:r><w:t>第一项</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);

    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");
    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].body, "第一项");
    assert!(!regions[0].skip_rewrite);
}

#[test]
fn imports_numbered_lists_as_visible_text_regions() {
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

    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");
    let rebuilt = regions
        .iter()
        .map(|region| region.body.as_str())
        .collect::<String>();

    assert!(rebuilt.contains("第一项"));
}

#[test]
fn imports_body_tables_as_placeholders() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:tbl>
      <w:tr>
        <w:tc>
          <w:p><w:r><w:t>表1 实验结果</w:t></w:r></w:p>
        </w:tc>
      </w:tr>
    </w:tbl>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);

    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");
    let rebuilt = regions
        .iter()
        .map(|region| region.body.as_str())
        .collect::<String>();

    assert!(rebuilt.contains("[表格"));
}

#[test]
fn keeps_formulas_visible_but_locked() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document
  xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
  xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math">
  <w:body>
    <w:p>
      <w:r><w:t>正文</w:t></w:r>
      <m:oMath>
        <m:r><m:t>E=mc^2</m:t></m:r>
      </m:oMath>
      <w:r><w:t>结论</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);

    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");
    assert!(regions
        .iter()
        .any(|region| region.skip_rewrite && region.body.contains("E=mc^2")));
}

#[test]
fn imports_page_breaks_as_placeholders() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:t>上文</w:t></w:r>
      <w:r><w:br w:type="page"/></w:r>
      <w:r><w:t>下文</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);

    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");
    let rebuilt = regions
        .iter()
        .map(|region| region.body.as_str())
        .collect::<String>();

    assert!(rebuilt.contains("[分页符]"));
}

#[test]
fn ignores_last_rendered_page_breaks_during_import() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r>
        <w:t>上文</w:t>
        <w:lastRenderedPageBreak/>
        <w:t>下文</w:t>
      </w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);

    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");
    let rebuilt = regions
        .iter()
        .map(|region| region.body.as_str())
        .collect::<String>();

    assert_eq!(rebuilt, "上文下文");
}

#[test]
fn imports_section_breaks_as_placeholders() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>正文</w:t></w:r></w:p>
    <w:sectPr/>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);

    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");
    let rebuilt = regions
        .iter()
        .map(|region| region.body.as_str())
        .collect::<String>();

    assert_eq!(rebuilt, "正文\n\n[分节符]");
}

#[test]
fn writes_back_docx_with_section_break_placeholder() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>正文</w:t></w:r></w:p>
    <w:sectPr/>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let source = DocxAdapter::extract_text(&bytes).expect("extract text");
    let writeback_source =
        DocxAdapter::extract_writeback_source_text(&bytes).expect("extract writeback source");
    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");

    assert_eq!(writeback_source, source);

    let rewritten = DocxAdapter::write_updated_regions(&bytes, &source, &regions)
        .expect("write updated regions");
    let extracted = DocxAdapter::extract_text(&rewritten).expect("extract rewritten text");

    assert_eq!(extracted, source);
}

#[test]
fn writes_back_docx_with_last_rendered_page_break() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r>
        <w:t>上文</w:t>
        <w:lastRenderedPageBreak/>
        <w:t>下文</w:t>
      </w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let source = DocxAdapter::extract_text(&bytes).expect("extract text");
    let writeback_source =
        DocxAdapter::extract_writeback_source_text(&bytes).expect("extract writeback source");
    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");

    assert_eq!(source, "上文下文");
    assert_eq!(writeback_source, source);
    DocxAdapter::validate_writeback(&bytes).expect("validate writeback");

    let rewritten = DocxAdapter::write_updated_regions(&bytes, &source, &regions)
        .expect("write updated regions");
    let extracted = DocxAdapter::extract_text(&rewritten).expect("extract rewritten text");

    assert_eq!(extracted, source);
}

#[test]
fn writes_back_docx_with_empty_paragraphs() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>第一段</w:t></w:r></w:p>
    <w:p></w:p>
    <w:p><w:r><w:t>第二段</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let source = DocxAdapter::extract_text(&bytes).expect("extract text");
    let writeback_source =
        DocxAdapter::extract_writeback_source_text(&bytes).expect("extract writeback source");
    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");

    assert_eq!(writeback_source, source);

    let rewritten = DocxAdapter::write_updated_regions(&bytes, &source, &regions)
        .expect("write updated regions");
    let extracted = DocxAdapter::extract_text(&rewritten).expect("extract rewritten text");

    assert_eq!(extracted, source);
}

#[test]
fn writes_back_docx_with_empty_paragraphs_after_chunk_roundtrip() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>第一段</w:t></w:r></w:p>
    <w:p></w:p>
    <w:p><w:r><w:t>第二段</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let source =
        DocxAdapter::extract_writeback_source_text(&bytes).expect("extract writeback source");
    let slots = DocxAdapter::extract_writeback_slots(&bytes, false).expect("extract slots");
    for preset in [
        SegmentationPreset::Clause,
        SegmentationPreset::Sentence,
        SegmentationPreset::Paragraph,
    ] {
        let _ = editable_unit_texts(&bytes, preset);
        let rewritten = DocxAdapter::write_updated_slots(&bytes, &source, &slots)
            .expect("write updated slots after segmentation roundtrip");
        let extracted = DocxAdapter::extract_text(&rewritten).expect("extract rewritten text");

        assert_eq!(extracted, source);
    }
}

#[test]
fn writes_back_docx_with_collapsed_empty_paragraph_separators() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>第一段</w:t></w:r></w:p>
    <w:p></w:p>
    <w:p><w:r><w:t>第二段</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let source =
        DocxAdapter::extract_writeback_source_text(&bytes).expect("extract writeback source");
    let slots = DocxAdapter::extract_writeback_slots(&bytes, false).expect("extract slots");
    let rewritten = DocxAdapter::write_updated_slots(&bytes, &source, &slots)
        .expect("write updated slots from collapsed separators");
    let extracted = DocxAdapter::extract_text(&rewritten).expect("extract rewritten text");

    assert_eq!(extracted, source);
}

#[test]
fn writes_back_docx_with_locked_heading_regions() {
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
    let source = DocxAdapter::extract_text(&bytes).expect("extract text");
    let writeback_source =
        DocxAdapter::extract_writeback_source_text(&bytes).expect("extract writeback source");
    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");

    assert_eq!(writeback_source, source);
    assert!(regions.iter().any(|region| region.skip_rewrite));

    let rewritten = DocxAdapter::write_updated_regions(&bytes, &source, &regions)
        .expect("write updated regions");
    let extracted = DocxAdapter::extract_text(&rewritten).expect("extract rewritten text");

    assert_eq!(extracted, source);
}

#[test]
fn writes_back_docx_with_adjacent_plain_runs() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:t>前半句，</w:t></w:r>
      <w:r><w:t>后半句。</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let source = DocxAdapter::extract_text(&bytes).expect("extract text");
    let writeback_source =
        DocxAdapter::extract_writeback_source_text(&bytes).expect("extract writeback source");
    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");

    assert_eq!(writeback_source, source);
    assert_eq!(regions.len(), 1);

    let rewritten = DocxAdapter::write_updated_regions(&bytes, &source, &regions)
        .expect("write updated regions");
    let extracted = DocxAdapter::extract_text(&rewritten).expect("extract rewritten text");

    assert_eq!(extracted, source);
}

