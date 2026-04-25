use super::*;

#[test]
fn rejects_writeback_when_source_text_mismatch() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>原文</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);

    let error = DocxAdapter::write_updated_text(&bytes, "不是原文", "新正文")
        .expect_err("expected mismatch failure");
    assert!(error.contains("已变化") || error.contains("不一致"));
}

#[test]
fn supports_common_inline_run_styles_during_import() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r>
        <w:rPr><w:b/></w:rPr>
        <w:t>粗体文本</w:t>
      </w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);

    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");
    let region = regions
        .iter()
        .find(|region| region.body.contains("粗体文本"))
        .expect("styled region");
    let presentation = region.presentation.as_ref().expect("presentation");

    assert!(presentation.bold);
    assert!(!region.skip_rewrite);
}

#[test]
fn imports_embedded_office_objects_as_locked_placeholder() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:object/></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);

    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");
    assert!(regions.iter().any(|region| {
        region.body.contains("[复杂结构:object]")
            && protect_kind_of(region) == Some("unknown-structure")
    }));
}

#[test]
fn extracts_run_style_presentation_from_docx() {
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

    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");
    let region = regions
        .iter()
        .find(|region| region.body.contains("样式文本"))
        .expect("styled region");
    let presentation = region.presentation.as_ref().expect("presentation");

    assert!(presentation.bold);
    assert!(presentation.italic);
    assert!(presentation.underline);
}

#[test]
fn extracts_hyperlink_display_text_with_target_presentation() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
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
        ("word/document.xml", xml),
        ("word/_rels/document.xml.rels", rels),
    ]);

    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");
    let region = regions
        .iter()
        .find(|region| region.body.contains("示例链接"))
        .expect("hyperlink region");
    let presentation = region.presentation.as_ref().expect("presentation");

    assert!(!region.skip_rewrite);
    assert_eq!(presentation.href.as_deref(), Some("https://example.com"));
}

#[test]
fn locks_bare_urls_inside_plain_docx_runs() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r>
        <w:t>访问 https://chat.deepseek.com/share/lzlvnjcj3o5uees841 查看答案</w:t>
      </w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);

    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");
    let parts = regions
        .iter()
        .map(|region| (region.body.as_str(), region.skip_rewrite))
        .collect::<Vec<_>>();

    assert_eq!(
        parts,
        vec![
            ("访问 ", false),
            ("https://chat.deepseek.com/share/lzlvnjcj3o5uees841", true),
            (" 查看答案", false),
        ]
    );
}

#[test]
fn keeps_url_with_trailing_space_as_one_locked_region() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r>
        <w:t xml:space="preserve">https://chat.deepseek.com/share/lzlvnjcj3o5uees841 </w:t>
      </w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);

    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");
    let parts = regions
        .iter()
        .map(|region| (region.body.as_str(), region.skip_rewrite))
        .collect::<Vec<_>>();

    assert_eq!(
        parts,
        vec![("https://chat.deepseek.com/share/lzlvnjcj3o5uees841 ", true)]
    );
}

#[test]
fn writes_back_repo_sample_docx_without_false_source_mismatch() {
    let bytes = build_chunk_test_fixture_docx();
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
fn imports_simple_fields_as_locked_visible_regions() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:t>前</w:t></w:r>
      <w:fldSimple w:instr=" FILENAME ">
        <w:r><w:t>文档名.docx</w:t></w:r>
      </w:fldSimple>
      <w:r><w:t>后</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);

    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");

    assert_eq!(joined_region_text(&regions), "前文档名.docx后");
    let field_region = regions
        .iter()
        .find(|region| region.body == "文档名.docx")
        .expect("field region");
    assert!(field_region.skip_rewrite);
    assert_eq!(protect_kind_of(field_region), Some("field"));
}

#[test]
fn roundtrips_simple_fields_through_writeback() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:t>前</w:t></w:r>
      <w:fldSimple w:instr=" AUTHOR ">
        <w:r><w:t>作者</w:t></w:r>
      </w:fldSimple>
      <w:r><w:t>后</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let source = DocxAdapter::extract_writeback_source_text(&bytes).expect("extract source");
    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");

    let rewritten = DocxAdapter::write_updated_regions(&bytes, &source, &regions)
        .expect("write updated regions");
    let extracted =
        DocxAdapter::extract_writeback_source_text(&rewritten).expect("extract rewritten source");

    assert_eq!(source, "前作者后");
    assert_eq!(extracted, source);
}

#[test]
fn imports_inline_content_controls_as_locked_placeholders() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:t>前</w:t></w:r>
      <w:sdt>
        <w:sdtPr><w:alias w:val="普通内容控件"/></w:sdtPr>
        <w:sdtContent>
          <w:r><w:t>控件内容</w:t></w:r>
        </w:sdtContent>
      </w:sdt>
      <w:r><w:t>后</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);

    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");

    assert_eq!(joined_region_text(&regions), "前[内容控件]后");
    assert!(regions.iter().any(|region| region.body == "[内容控件]"
        && region.skip_rewrite
        && protect_kind_of(region) == Some("content-control")));
}

#[test]
fn imports_block_content_controls_as_content_control_placeholders() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>前</w:t></w:r></w:p>
    <w:sdt>
      <w:sdtPr><w:alias w:val="普通内容控件"/></w:sdtPr>
      <w:sdtContent>
        <w:p><w:r><w:t>控件内容</w:t></w:r></w:p>
      </w:sdtContent>
    </w:sdt>
    <w:p><w:r><w:t>后</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);

    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");

    assert_eq!(joined_region_text(&regions), "前\n\n[内容控件]\n\n后");
    assert!(regions
        .iter()
        .any(|region| region.body.starts_with("[内容控件]")
            && region.skip_rewrite
            && protect_kind_of(region) == Some("content-control")));
}

#[test]
fn imports_run_special_characters_and_roundtrips_writeback() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:t>甲</w:t></w:r>
      <w:r><w:noBreakHyphen/></w:r>
      <w:r><w:t>乙</w:t></w:r>
      <w:r><w:softHyphen/></w:r>
      <w:r><w:t>丙</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let expected = format!("甲{}乙\u{00ad}丙", '\u{2011}');

    let source = DocxAdapter::extract_writeback_source_text(&bytes).expect("extract source");
    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");
    let rewritten = DocxAdapter::write_updated_regions(&bytes, &source, &regions)
        .expect("write updated regions");
    let extracted =
        DocxAdapter::extract_writeback_source_text(&rewritten).expect("extract rewritten source");

    assert_eq!(source, expected);
    assert_eq!(joined_region_text(&regions), expected);
    assert_eq!(extracted, expected);
}

#[test]
fn imports_numbering_start_override_markers() {
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:pPr>
        <w:numPr>
          <w:ilvl w:val="0"/>
          <w:numId w:val="9"/>
        </w:numPr>
      </w:pPr>
      <w:r><w:t>覆盖起始值</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let numbering_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:abstractNum w:abstractNumId="0">
    <w:lvl w:ilvl="0">
      <w:start w:val="1"/>
      <w:numFmt w:val="decimal"/>
      <w:lvlText w:val="%1."/>
      <w:suff w:val="space"/>
    </w:lvl>
  </w:abstractNum>
  <w:num w:numId="9">
    <w:abstractNumId w:val="0"/>
    <w:lvlOverride w:ilvl="0">
      <w:startOverride w:val="5"/>
    </w:lvlOverride>
  </w:num>
</w:numbering>"#;
    let bytes = build_docx_entries(&[
        ("word/document.xml", document_xml),
        ("word/numbering.xml", numbering_xml),
    ]);

    let text = DocxAdapter::extract_text(&bytes).expect("extract text");

    assert_eq!(text, "5. 覆盖起始值");
}

#[test]
fn imports_numbering_from_level_paragraph_style_binding() {
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:pPr><w:pStyle w:val="CustomHeading"/></w:pPr>
      <w:r><w:t>作品概述</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let styles_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:style w:type="paragraph" w:styleId="CustomHeading">
    <w:name w:val="custom heading"/>
  </w:style>
</w:styles>"#;
    let numbering_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:abstractNum w:abstractNumId="0">
    <w:lvl w:ilvl="0">
      <w:start w:val="1"/>
      <w:numFmt w:val="decimal"/>
      <w:pStyle w:val="CustomHeading"/>
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

    assert_eq!(text, "第1章 作品概述");
}
