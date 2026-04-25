use super::*;

#[test]
fn keeps_distinct_regions_for_sibling_hyperlinks_with_same_target() {
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
            xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <w:body>
    <w:p>
      <w:hyperlink r:id="rId1"><w:r><w:t>甲</w:t></w:r></w:hyperlink>
      <w:hyperlink r:id="rId2"><w:r><w:t>乙</w:t></w:r></w:hyperlink>
    </w:p>
  </w:body>
</w:document>"#;
    let relationships_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1"
    Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink"
    Target="https://example.com"
    TargetMode="External"/>
  <Relationship Id="rId2"
    Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink"
    Target="https://example.com"
    TargetMode="External"/>
</Relationships>"#;
    let bytes = build_docx_entries(&[
        ("word/document.xml", document_xml),
        ("word/_rels/document.xml.rels", relationships_xml),
    ]);
    let source = DocxAdapter::extract_text(&bytes).expect("extract text");
    let writeback_source =
        DocxAdapter::extract_writeback_source_text(&bytes).expect("extract writeback source");
    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");

    assert_eq!(writeback_source, source);
    assert_eq!(regions.len(), 2);
    assert_ne!(regions[0].presentation, regions[1].presentation);

    let rewritten = DocxAdapter::write_updated_regions(&bytes, &source, &regions)
        .expect("write updated regions");
    let extracted = DocxAdapter::extract_text(&rewritten).expect("extract rewritten text");

    assert_eq!(extracted, source);
}

#[test]
fn writes_back_hyperlink_with_tab_and_line_break_inside() {
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
            xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <w:body>
    <w:p>
      <w:hyperlink r:id="rId1">
        <w:r><w:t>甲</w:t></w:r>
        <w:r><w:tab/></w:r>
        <w:r><w:t>乙</w:t></w:r>
        <w:r><w:br/></w:r>
        <w:r><w:t>丙</w:t></w:r>
      </w:hyperlink>
    </w:p>
  </w:body>
</w:document>"#;
    let relationships_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1"
    Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink"
    Target="https://example.com"
    TargetMode="External"/>
</Relationships>"#;
    let bytes = build_docx_entries(&[
        ("word/document.xml", document_xml),
        ("word/_rels/document.xml.rels", relationships_xml),
    ]);
    let source = DocxAdapter::extract_text(&bytes).expect("extract text");
    let writeback_source =
        DocxAdapter::extract_writeback_source_text(&bytes).expect("extract writeback source");
    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");

    assert_eq!(writeback_source, source);
    assert_eq!(joined_region_text(&regions), "甲\t乙\n丙");
    assert_eq!(
        regions[0]
            .presentation
            .as_ref()
            .and_then(|presentation| presentation.href.as_deref()),
        Some("https://example.com")
    );

    let rewritten = DocxAdapter::write_updated_regions(&bytes, &source, &regions)
        .expect("write updated regions");
    let extracted = DocxAdapter::extract_text(&rewritten).expect("extract rewritten text");

    assert_eq!(extracted, source);
}

#[test]
fn imports_hyperlink_with_embedded_formula_as_locked_placeholder() {
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document
  xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
  xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
  xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math">
  <w:body>
    <w:p>
      <w:hyperlink r:id="rId1">
        <m:oMath><m:r><m:t>E=mc^2</m:t></m:r></m:oMath>
      </w:hyperlink>
    </w:p>
  </w:body>
</w:document>"#;
    let relationships_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1"
    Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink"
    Target="https://example.com"
    TargetMode="External"/>
</Relationships>"#;
    let bytes = build_docx_entries(&[
        ("word/document.xml", document_xml),
        ("word/_rels/document.xml.rels", relationships_xml),
    ]);

    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");
    assert!(regions.iter().any(|region| {
        region.body.contains("[复杂结构:hyperlink]")
            && protect_kind_of(region) == Some("unknown-structure")
    }));

    let source = DocxAdapter::extract_writeback_source_text(&bytes).expect("extract source");
    let rewritten = DocxAdapter::write_updated_regions(&bytes, &source, &regions)
        .expect("write updated regions");
    let extracted = DocxAdapter::extract_writeback_source_text(&rewritten).expect("extract source");
    assert_eq!(extracted, source);
}

#[test]
fn imports_hyperlink_with_page_break_as_locked_placeholder() {
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
            xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <w:body>
    <w:p>
      <w:hyperlink r:id="rId1">
        <w:r><w:t>甲</w:t></w:r>
        <w:r><w:br w:type="page"/></w:r>
        <w:r><w:t>乙</w:t></w:r>
      </w:hyperlink>
    </w:p>
  </w:body>
</w:document>"#;
    let relationships_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1"
    Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink"
    Target="https://example.com"
    TargetMode="External"/>
</Relationships>"#;
    let bytes = build_docx_entries(&[
        ("word/document.xml", document_xml),
        ("word/_rels/document.xml.rels", relationships_xml),
    ]);

    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");
    assert!(regions.iter().any(|region| {
        region.body.contains("[复杂结构:hyperlink]")
            && protect_kind_of(region) == Some("unknown-structure")
    }));

    let source = DocxAdapter::extract_writeback_source_text(&bytes).expect("extract source");
    let rewritten = DocxAdapter::write_updated_regions(&bytes, &source, &regions)
        .expect("write updated regions");
    let extracted = DocxAdapter::extract_writeback_source_text(&rewritten).expect("extract source");
    assert_eq!(extracted, source);
}

