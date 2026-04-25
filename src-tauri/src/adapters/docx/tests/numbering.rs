use super::*;

#[test]
fn imports_numbering_prefixes_as_locked_visible_regions() {
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:pPr>
        <w:numPr>
          <w:ilvl w:val="0"/>
          <w:numId w:val="7"/>
        </w:numPr>
      </w:pPr>
      <w:r><w:t>填写日期</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let numbering_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:abstractNum w:abstractNumId="3">
    <w:lvl w:ilvl="0">
      <w:start w:val="1"/>
      <w:numFmt w:val="decimal"/>
      <w:lvlText w:val="%1、"/>
    </w:lvl>
  </w:abstractNum>
  <w:num w:numId="7">
    <w:abstractNumId w:val="3"/>
  </w:num>
</w:numbering>"#;
    let bytes = build_docx_entries(&[
        ("word/document.xml", document_xml),
        ("word/numbering.xml", numbering_xml),
    ]);

    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");
    let rebuilt = joined_region_text(&regions);

    assert_eq!(rebuilt, "1、填写日期");
    assert_eq!(
        regions.first().map(|region| region.body.as_str()),
        Some("1、")
    );
    assert!(regions.first().is_some_and(|region| region.skip_rewrite));
    assert_eq!(
        regions.first().and_then(|region| protect_kind_of(region)),
        Some("list-marker")
    );
}

#[test]
fn imports_style_inherited_heading_numbering_as_locked_visible_prefixes() {
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:pPr><w:pStyle w:val="1"/></w:pPr>
      <w:r><w:t>作品概述</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let styles_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:style w:type="paragraph" w:styleId="1">
    <w:name w:val="heading 1"/>
    <w:pPr>
      <w:numPr><w:numId w:val="1"/></w:numPr>
    </w:pPr>
  </w:style>
</w:styles>"#;
    let numbering_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:abstractNum w:abstractNumId="0">
    <w:lvl w:ilvl="0">
      <w:start w:val="1"/>
      <w:numFmt w:val="decimal"/>
      <w:lvlText w:val="第%1章"/>
      <w:suff w:val="space"/>
    </w:lvl>
  </w:abstractNum>
  <w:num w:numId="1">
    <w:abstractNumId w:val="0"/>
  </w:num>
</w:numbering>"#;
    let bytes = build_docx_entries(&[
        ("word/document.xml", document_xml),
        ("word/styles.xml", styles_xml),
        ("word/numbering.xml", numbering_xml),
    ]);

    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");
    let rebuilt = joined_region_text(&regions);

    assert_eq!(rebuilt, "第1章 作品概述");
    assert_eq!(
        regions.first().map(|region| region.body.as_str()),
        Some("第1章 ")
    );
    assert!(regions.first().is_some_and(|region| region.skip_rewrite));
    assert_eq!(
        regions.first().and_then(protect_kind_of),
        Some("list-marker")
    );
}

#[test]
fn imports_style_numbering_through_based_on_chain() {
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:pPr><w:pStyle w:val="DerivedHeading"/></w:pPr>
      <w:r><w:t>二级标题示例</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let styles_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:style w:type="paragraph" w:styleId="BaseHeading">
    <w:name w:val="heading 1"/>
    <w:pPr><w:numPr><w:numId w:val="1"/></w:numPr></w:pPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="DerivedHeading">
    <w:basedOn w:val="BaseHeading"/>
    <w:pPr><w:numPr><w:ilvl w:val="1"/></w:numPr></w:pPr>
  </w:style>
</w:styles>"#;
    let numbering_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:abstractNum w:abstractNumId="0">
    <w:lvl w:ilvl="0">
      <w:start w:val="1"/>
      <w:numFmt w:val="decimal"/>
      <w:lvlText w:val="第%1章"/>
      <w:suff w:val="space"/>
    </w:lvl>
    <w:lvl w:ilvl="1">
      <w:start w:val="1"/>
      <w:numFmt w:val="decimal"/>
      <w:lvlText w:val="%1.%2"/>
      <w:suff w:val="space"/>
    </w:lvl>
  </w:abstractNum>
  <w:num w:numId="1">
    <w:abstractNumId w:val="0"/>
  </w:num>
</w:numbering>"#;
    let bytes = build_docx_entries(&[
        ("word/document.xml", document_xml),
        ("word/styles.xml", styles_xml),
        ("word/numbering.xml", numbering_xml),
    ]);

    let text = DocxAdapter::extract_text(&bytes).expect("extract text");

    assert_eq!(text, "1.1 二级标题示例");
}

#[test]
fn imports_multilevel_style_numbering_with_full_marker_text() {
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:pPr><w:pStyle w:val="1"/></w:pPr><w:r><w:t>作品概述</w:t></w:r></w:p>
    <w:p><w:pPr><w:pStyle w:val="2"/></w:pPr><w:r><w:t>二级标题示例</w:t></w:r></w:p>
    <w:p><w:pPr><w:pStyle w:val="3"/></w:pPr><w:r><w:t>三级标题示例</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
    let styles_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:style w:type="paragraph" w:styleId="1">
    <w:pPr><w:numPr><w:numId w:val="1"/></w:numPr></w:pPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="2">
    <w:pPr><w:numPr><w:ilvl w:val="1"/><w:numId w:val="1"/></w:numPr></w:pPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="3">
    <w:pPr><w:numPr><w:ilvl w:val="2"/><w:numId w:val="1"/></w:numPr></w:pPr>
  </w:style>
</w:styles>"#;
    let numbering_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:abstractNum w:abstractNumId="0">
    <w:lvl w:ilvl="0">
      <w:start w:val="1"/>
      <w:numFmt w:val="decimal"/>
      <w:lvlText w:val="第%1章"/>
      <w:suff w:val="space"/>
    </w:lvl>
    <w:lvl w:ilvl="1">
      <w:start w:val="1"/>
      <w:numFmt w:val="decimal"/>
      <w:lvlText w:val="%1.%2"/>
      <w:suff w:val="space"/>
    </w:lvl>
    <w:lvl w:ilvl="2">
      <w:start w:val="1"/>
      <w:numFmt w:val="decimal"/>
      <w:lvlText w:val="%1.%2.%3"/>
      <w:suff w:val="space"/>
    </w:lvl>
  </w:abstractNum>
  <w:num w:numId="1">
    <w:abstractNumId w:val="0"/>
  </w:num>
</w:numbering>"#;
    let bytes = build_docx_entries(&[
        ("word/document.xml", document_xml),
        ("word/styles.xml", styles_xml),
        ("word/numbering.xml", numbering_xml),
    ]);

    let text = DocxAdapter::extract_text(&bytes).expect("extract text");

    assert_eq!(
        text,
        "第1章 作品概述\n\n1.1 二级标题示例\n\n1.1.1 三级标题示例"
    );
}

#[test]
fn does_not_consume_heading_numbering_on_empty_style_numbered_paragraph() {
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:pPr><w:pStyle w:val="1"/></w:pPr></w:p>
    <w:p><w:pPr><w:pStyle w:val="1"/></w:pPr><w:r><w:t>作品概述</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
    let styles_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:style w:type="paragraph" w:styleId="1">
    <w:name w:val="heading 1"/>
    <w:pPr><w:numPr><w:numId w:val="1"/></w:numPr></w:pPr>
  </w:style>
</w:styles>"#;
    let numbering_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:abstractNum w:abstractNumId="0">
    <w:lvl w:ilvl="0">
      <w:start w:val="1"/>
      <w:numFmt w:val="decimal"/>
      <w:lvlText w:val="第%1章"/>
      <w:suff w:val="space"/>
    </w:lvl>
  </w:abstractNum>
  <w:num w:numId="1">
    <w:abstractNumId w:val="0"/>
  </w:num>
</w:numbering>"#;
    let bytes = build_docx_entries(&[
        ("word/document.xml", document_xml),
        ("word/styles.xml", styles_xml),
        ("word/numbering.xml", numbering_xml),
    ]);

    let text = DocxAdapter::extract_text(&bytes).expect("extract text");

    assert_eq!(text, "\n\n第1章 作品概述");
}

#[test]
fn imports_numbering_when_paragraph_direct_numpr_completes_style_numpr() {
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:pPr>
        <w:pStyle w:val="BodyNumbered"/>
        <w:numPr><w:ilvl w:val="1"/></w:numPr>
      </w:pPr>
      <w:r><w:t>二级正文</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let styles_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:style w:type="paragraph" w:styleId="BodyNumbered">
    <w:pPr><w:numPr><w:numId w:val="1"/></w:numPr></w:pPr>
  </w:style>
</w:styles>"#;
    let numbering_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:abstractNum w:abstractNumId="0">
    <w:lvl w:ilvl="0">
      <w:start w:val="1"/>
      <w:numFmt w:val="decimal"/>
      <w:lvlText w:val="%1"/>
      <w:suff w:val="space"/>
    </w:lvl>
    <w:lvl w:ilvl="1">
      <w:start w:val="1"/>
      <w:numFmt w:val="decimal"/>
      <w:lvlText w:val="%1.%2"/>
      <w:suff w:val="space"/>
    </w:lvl>
  </w:abstractNum>
  <w:num w:numId="1">
    <w:abstractNumId w:val="0"/>
  </w:num>
</w:numbering>"#;
    let bytes = build_docx_entries(&[
        ("word/document.xml", document_xml),
        ("word/styles.xml", styles_xml),
        ("word/numbering.xml", numbering_xml),
    ]);

    let text = DocxAdapter::extract_text(&bytes).expect("extract text");

    assert_eq!(text, "1.1 二级正文");
}

#[test]
fn imports_numbering_suffix_tabs_as_visible_locked_text() {
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:pPr>
        <w:numPr>
          <w:ilvl w:val="0"/>
          <w:numId w:val="7"/>
        </w:numPr>
      </w:pPr>
      <w:r><w:t>填写日期</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let numbering_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:abstractNum w:abstractNumId="3">
    <w:lvl w:ilvl="0">
      <w:start w:val="1"/>
      <w:numFmt w:val="decimal"/>
      <w:lvlText w:val="%1."/>
      <w:suff w:val="tab"/>
    </w:lvl>
  </w:abstractNum>
  <w:num w:numId="7">
    <w:abstractNumId w:val="3"/>
  </w:num>
</w:numbering>"#;
    let bytes = build_docx_entries(&[
        ("word/document.xml", document_xml),
        ("word/numbering.xml", numbering_xml),
    ]);

    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");
    let rebuilt = joined_region_text(&regions);

    assert_eq!(rebuilt, "1.\t填写日期");
    assert_eq!(
        regions.first().map(|region| region.body.as_str()),
        Some("1.\t")
    );
}

#[test]
fn marks_style_named_heading_regions_as_skip_regions_by_default() {
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:pPr><w:pStyle w:val="1"/></w:pPr>
      <w:r><w:t>作品概述</w:t></w:r>
    </w:p>
    <w:p><w:r><w:t>正文</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
    let styles_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:style w:type="paragraph" w:styleId="1">
    <w:pPr><w:outlineLvl w:val="0"/></w:pPr>
  </w:style>
</w:styles>"#;
    let bytes = build_docx_entries(&[
        ("word/document.xml", document_xml),
        ("word/styles.xml", styles_xml),
    ]);

    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");

    assert!(regions
        .iter()
        .any(|region| region.skip_rewrite && region.body.contains("作品概述")));
}

#[test]
fn imports_floating_textboxes_as_following_locked_blocks_in_reading_order() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
            xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
            xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
            xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">
  <w:body>
    <w:p>
      <w:r>
        <w:drawing>
          <wp:anchor>
            <wp:positionV relativeFrom="page"><wp:posOffset>2400</wp:posOffset></wp:positionV>
            <a:graphic>
              <a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">
                <wps:wsp>
                  <wps:txbx>
                    <w:txbxContent>
                      <w:p><w:r><w:t>填写说明</w:t></w:r></w:p>
                    </w:txbxContent>
                  </wps:txbx>
                </wps:wsp>
              </a:graphicData>
            </a:graphic>
          </wp:anchor>
        </w:drawing>
      </w:r>
      <w:r><w:t>填写日期：</w:t></w:r>
    </w:p>
    <w:p><w:r><w:t>后续正文</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);

    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");
    let rebuilt = joined_region_text(&regions);

    assert_eq!(rebuilt, "填写日期：\n\n[文本框]\n\n后续正文");
    assert!(!rebuilt.contains("[文本框]填写日期："));
}

#[test]
fn writes_back_numbered_paragraphs_with_visible_locked_prefixes() {
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:pPr>
        <w:numPr>
          <w:ilvl w:val="0"/>
          <w:numId w:val="7"/>
        </w:numPr>
      </w:pPr>
      <w:r><w:t>填写日期</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let numbering_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:abstractNum w:abstractNumId="3">
    <w:lvl w:ilvl="0">
      <w:start w:val="1"/>
      <w:numFmt w:val="decimal"/>
      <w:lvlText w:val="%1、"/>
    </w:lvl>
  </w:abstractNum>
  <w:num w:numId="7">
    <w:abstractNumId w:val="3"/>
  </w:num>
</w:numbering>"#;
    let bytes = build_docx_entries(&[
        ("word/document.xml", document_xml),
        ("word/numbering.xml", numbering_xml),
    ]);
    let source = DocxAdapter::extract_writeback_source_text(&bytes).expect("extract source");

    let rewritten = DocxAdapter::write_updated_text(&bytes, &source, "1、改写日期")
        .expect("write updated text");
    let extracted = DocxAdapter::extract_text(&rewritten).expect("extract rewritten text");

    assert_eq!(source, "1、填写日期");
    assert_eq!(extracted, "1、改写日期");
}

#[test]
fn writes_back_style_inherited_heading_numbering_without_losing_prefixes() {
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:pPr><w:pStyle w:val="1"/></w:pPr><w:r><w:t>作品概述</w:t></w:r></w:p>
    <w:p><w:pPr><w:pStyle w:val="2"/></w:pPr><w:r><w:t>二级标题示例</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
    let styles_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:style w:type="paragraph" w:styleId="1">
    <w:pPr><w:numPr><w:numId w:val="1"/></w:numPr></w:pPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="2">
    <w:pPr><w:numPr><w:ilvl w:val="1"/><w:numId w:val="1"/></w:numPr></w:pPr>
  </w:style>
</w:styles>"#;
    let numbering_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:abstractNum w:abstractNumId="0">
    <w:lvl w:ilvl="0">
      <w:start w:val="1"/>
      <w:numFmt w:val="decimal"/>
      <w:lvlText w:val="第%1章"/>
      <w:suff w:val="space"/>
    </w:lvl>
    <w:lvl w:ilvl="1">
      <w:start w:val="1"/>
      <w:numFmt w:val="decimal"/>
      <w:lvlText w:val="%1.%2"/>
      <w:suff w:val="space"/>
    </w:lvl>
  </w:abstractNum>
  <w:num w:numId="1">
    <w:abstractNumId w:val="0"/>
  </w:num>
</w:numbering>"#;
    let bytes = build_docx_entries(&[
        ("word/document.xml", document_xml),
        ("word/styles.xml", styles_xml),
        ("word/numbering.xml", numbering_xml),
    ]);
    let source = DocxAdapter::extract_writeback_source_text(&bytes).expect("extract source");

    let rewritten = DocxAdapter::write_updated_text(
        &bytes,
        &source,
        "第1章 改写后的作品概述\n\n1.1 改写后的二级标题",
    )
    .expect("write updated text");
    let extracted = DocxAdapter::extract_text(&rewritten).expect("extract rewritten text");

    assert_eq!(source, "第1章 作品概述\n\n1.1 二级标题示例");
    assert_eq!(extracted, "第1章 改写后的作品概述\n\n1.1 改写后的二级标题");
}

#[test]
fn writes_back_text_around_floating_textboxes_in_display_order() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
            xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
            xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
            xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">
  <w:body>
    <w:p>
      <w:r>
        <w:drawing>
          <wp:anchor>
            <wp:positionV relativeFrom="page"><wp:posOffset>2400</wp:posOffset></wp:positionV>
            <a:graphic>
              <a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">
                <wps:wsp>
                  <wps:txbx>
                    <w:txbxContent>
                      <w:p><w:r><w:t>填写说明</w:t></w:r></w:p>
                    </w:txbxContent>
                  </wps:txbx>
                </wps:wsp>
              </a:graphicData>
            </a:graphic>
          </wp:anchor>
        </w:drawing>
      </w:r>
      <w:r><w:t>填写日期：</w:t></w:r>
    </w:p>
    <w:p><w:r><w:t>后续正文</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let source = DocxAdapter::extract_writeback_source_text(&bytes).expect("extract source");

    let rewritten =
        DocxAdapter::write_updated_text(&bytes, &source, "改写日期：\n\n[文本框]\n\n改写后的正文")
            .expect("write updated text");
    let extracted = DocxAdapter::extract_text(&rewritten).expect("extract rewritten text");

    assert_eq!(source, "填写日期：\n\n[文本框]\n\n后续正文");
    assert_eq!(extracted, "改写日期：\n\n[文本框]\n\n改写后的正文");
}

