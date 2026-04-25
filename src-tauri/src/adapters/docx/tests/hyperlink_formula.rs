use super::*;

#[test]
fn validates_writeback_for_docx_with_hyperlinks() {
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
            xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <w:body>
    <w:p>
      <w:hyperlink r:id="rId1">
        <w:r><w:t>示例链接</w:t></w:r>
      </w:hyperlink>
    </w:p>
  </w:body>
</w:document>"#;
    let relationships_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship
    Id="rId1"
    Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink"
    Target="https://example.com"
    TargetMode="External"/>
</Relationships>"#;

    let bytes = build_docx_entries(&[
        ("word/document.xml", document_xml),
        ("word/_rels/document.xml.rels", relationships_xml),
    ]);
    DocxAdapter::validate_writeback(&bytes).expect("writeback should be allowed");
}

#[test]
fn writes_back_hyperlink_display_text_without_touching_url() {
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
            xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <w:body>
    <w:p>
      <w:r><w:t>访问</w:t></w:r>
      <w:hyperlink r:id="rId5">
        <w:r><w:t>示例链接</w:t></w:r>
      </w:hyperlink>
    </w:p>
  </w:body>
</w:document>"#;
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId5"
                Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink"
                Target="https://example.com"
                TargetMode="External"/>
</Relationships>"#;
    let bytes = build_docx_entries(&[
        ("word/document.xml", document_xml),
        ("word/_rels/document.xml.rels", rels),
    ]);
    let source = DocxAdapter::extract_text(&bytes).expect("extract text");
    let imported_regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");
    let hyperlink_presentation = imported_regions
        .iter()
        .find(|region| region.body == "示例链接")
        .and_then(|region| region.presentation.clone())
        .expect("hyperlink presentation");
    let updated_regions = vec![
        TextRegion::editable("访问"),
        TextRegion::editable("新版链接").with_presentation(Some(hyperlink_presentation)),
    ];

    let rewritten = DocxAdapter::write_updated_regions(&bytes, &source, &updated_regions)
        .expect("write updated regions");
    let extracted = DocxAdapter::extract_text(&rewritten).expect("extract updated text");
    let rewritten_document_xml = read_docx_entry(&rewritten, "word/document.xml");
    let rewritten_rels = read_docx_entry(&rewritten, "word/_rels/document.xml.rels");

    assert_eq!(extracted, "访问新版链接");
    assert!(rewritten_document_xml.contains("w:hyperlink"));
    assert!(rewritten_rels.contains("https://example.com"));
}

#[test]
fn writes_back_styled_run_text_without_dropping_run_properties() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r>
        <w:rPr>
          <w:b/>
          <w:i/>
          <w:u w:val="single"/>
        </w:rPr>
        <w:t>样式文本</w:t>
      </w:r>
    </w:p>
  </w:body>
    </w:document>"#;
    let bytes = build_minimal_docx(xml);
    let source = DocxAdapter::extract_text(&bytes).expect("extract text");
    let imported_regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");
    let writeback_regions =
        DocxAdapter::extract_writeback_regions(&bytes).expect("extract writeback regions");
    let presentation = imported_regions
        .iter()
        .find(|region| region.body == "样式文本")
        .and_then(|region| region.presentation.clone())
        .expect("styled presentation");
    let writeback_presentation = writeback_regions
        .iter()
        .find(|region| region.body == "样式文本")
        .and_then(|region| region.presentation.clone())
        .expect("writeback presentation");
    assert_eq!(presentation, writeback_presentation);
    let updated_regions =
        vec![TextRegion::editable("更新样式文本").with_presentation(Some(presentation))];

    let rewritten = DocxAdapter::write_updated_regions(&bytes, &source, &updated_regions)
        .expect("write updated regions");
    let extracted = DocxAdapter::extract_regions(&rewritten, false).expect("extract regions");
    let document_xml = read_docx_entry(&rewritten, "word/document.xml");
    let region = extracted
        .iter()
        .find(|region| region.body == "更新样式文本")
        .expect("styled region");
    let presentation = region.presentation.as_ref().expect("presentation");

    assert!(presentation.bold);
    assert!(presentation.italic);
    assert!(presentation.underline);
    assert!(document_xml.contains("<w:b/>"));
    assert!(document_xml.contains("<w:i/>"));
    assert!(document_xml.contains("<w:u"));
}

#[test]
fn writes_back_regions_around_locked_formula_without_touching_formula_xml() {
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
    let source = DocxAdapter::extract_text(&bytes).expect("extract text");
    let updated_regions = vec![
        TextRegion::editable("更新正文"),
        TextRegion::inline_object("E=mc^2").with_presentation(Some(TextPresentation {
            bold: false,
            italic: false,
            underline: false,
            href: None,
            protect_kind: Some("formula".to_string()),
            writeback_key: None,
        })),
        TextRegion::editable("更新结论"),
    ];

    let rewritten = DocxAdapter::write_updated_regions(&bytes, &source, &updated_regions)
        .expect("write updated regions");
    let document_xml = read_docx_entry(&rewritten, "word/document.xml");
    let regions = DocxAdapter::extract_regions(&rewritten, false).expect("extract regions");

    assert!(document_xml.contains("<m:oMath>"));
    assert!(regions
        .iter()
        .any(|region| region.skip_rewrite && region.body.contains("E=mc^2")));
    assert!(regions
        .iter()
        .any(|region| region.body.contains("更新正文")));
    assert!(regions
        .iter()
        .any(|region| region.body.contains("更新结论")));
}

#[test]
fn writes_back_docx_with_adjacent_locked_formula_regions() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document
  xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
  xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math">
  <w:body>
    <w:p>
      <w:r><w:t>正文</w:t></w:r>
      <m:oMath>
        <m:r><m:t>x</m:t></m:r>
      </m:oMath>
      <m:oMath>
        <m:r><m:t>y</m:t></m:r>
      </m:oMath>
      <w:r><w:t>结论</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let source =
        DocxAdapter::extract_writeback_source_text(&bytes).expect("extract writeback source");
    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");

    let rewritten = DocxAdapter::write_updated_regions(&bytes, &source, &regions)
        .expect("write updated regions");
    let extracted = DocxAdapter::extract_writeback_source_text(&rewritten)
        .expect("extract rewritten writeback source");

    assert_eq!(extracted, source);
}

#[test]
fn writes_back_docx_with_adjacent_locked_formula_regions_after_chunk_roundtrip() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document
  xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
  xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math">
  <w:body>
    <w:p>
      <w:r><w:t>正文第一句。</w:t></w:r>
      <m:oMath>
        <m:r><m:t>x</m:t></m:r>
      </m:oMath>
      <m:oMath>
        <m:r><m:t>y</m:t></m:r>
      </m:oMath>
      <w:r><w:t>正文第二句。</w:t></w:r>
    </w:p>
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
        let extracted = DocxAdapter::extract_writeback_source_text(&rewritten)
            .expect("extract rewritten writeback source");

        assert_eq!(extracted, source);
    }
}

#[test]
fn preserves_locked_formula_when_region_writeback_mutates_formula_text() {
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
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let source = DocxAdapter::extract_text(&bytes).expect("extract text");
    let updated_regions = vec![
        TextRegion::editable("正文"),
        TextRegion::inline_object("被改坏的公式").with_presentation(Some(TextPresentation {
            bold: false,
            italic: false,
            underline: false,
            href: None,
            protect_kind: Some("formula".to_string()),
            writeback_key: None,
        })),
    ];

    let rewritten = DocxAdapter::write_updated_regions(&bytes, &source, &updated_regions)
        .expect("writeback should preserve locked formula");
    let extracted = DocxAdapter::extract_text(&rewritten).expect("extract rewritten text");
    let document_xml = read_docx_entry(&rewritten, "word/document.xml");

    assert_eq!(extracted, "正文E=mc^2");
    assert!(!extracted.contains("被改坏的公式"));
    assert!(document_xml.contains("<m:oMath>"));
}
