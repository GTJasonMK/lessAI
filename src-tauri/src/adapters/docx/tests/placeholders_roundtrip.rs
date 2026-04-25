use super::*;

#[test]
fn imports_chart_shape_and_group_shape_as_locked_placeholders() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
            xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
            xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
            xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
            xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"
            xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"
            xmlns:wpg="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup">
  <w:body>
    <w:p>
      <w:r><w:t>图前</w:t></w:r>
      <w:r>
        <w:drawing>
          <wp:inline>
            <a:graphic>
              <a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart">
                <c:chart r:id="rIdChart1"/>
              </a:graphicData>
            </a:graphic>
          </wp:inline>
        </w:drawing>
      </w:r>
      <w:r><w:t>图后</w:t></w:r>
    </w:p>
    <w:p>
      <w:r>
        <w:drawing>
          <wp:inline>
            <a:graphic>
              <a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">
                <wps:wsp/>
              </a:graphicData>
            </a:graphic>
          </wp:inline>
        </w:drawing>
      </w:r>
    </w:p>
    <w:p>
      <w:r>
        <w:drawing>
          <wp:inline>
            <a:graphic>
              <a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup">
                <wpg:wgp/>
              </a:graphicData>
            </a:graphic>
          </wp:inline>
        </w:drawing>
      </w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);

    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");

    assert!(regions
        .iter()
        .any(|region| region.body.contains("[图表]") && protect_kind_of(region) == Some("chart")));
    assert!(regions
        .iter()
        .any(|region| region.body.contains("[图形]") && protect_kind_of(region) == Some("shape")));
    assert!(regions.iter().any(|region| {
        region.body.contains("[组合图形]") && protect_kind_of(region) == Some("group-shape")
    }));
}

#[test]
fn imports_paragraph_level_drawing_as_locked_placeholder() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
            xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
            xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
            xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
            xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart">
  <w:body>
    <w:p>
      <w:r><w:t>图前</w:t></w:r>
      <w:drawing>
        <wp:inline>
          <a:graphic>
            <a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart">
              <c:chart r:id="rIdChart1"/>
            </a:graphicData>
          </a:graphic>
        </wp:inline>
      </w:drawing>
      <w:r><w:t>图后</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);

    let extracted = DocxAdapter::extract_text(&bytes).expect("extract text");
    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");

    assert_eq!(extracted, "图前[图表]图后");
    assert!(regions
        .iter()
        .any(|region| region.body == "[图表]" && protect_kind_of(region) == Some("chart")));
}

#[test]
fn imports_vml_pict_shapes_as_locked_shape_placeholders() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
            xmlns:v="urn:schemas-microsoft-com:vml">
  <w:body>
    <w:p>
      <w:r><w:t>图前</w:t></w:r>
      <w:r>
        <w:pict>
          <v:rect style="width:0;height:1.5pt"/>
        </w:pict>
      </w:r>
      <w:r><w:t>图后</w:t></w:r>
    </w:p>
    <w:p>
      <w:r>
        <w:pict>
          <v:roundrect style="height:77.85pt;width:158.15pt"/>
        </w:pict>
      </w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);

    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");
    let shape_placeholders = regions
        .iter()
        .filter(|region| region.body == "[图形]" && protect_kind_of(region) == Some("shape"))
        .count();

    assert_eq!(shape_placeholders, 2);
}

#[test]
fn imports_unknown_body_structure_as_locked_placeholder_instead_of_rejecting() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
            xmlns:x="urn:lessai:unknown-structure">
  <w:body>
    <x:customBlock>
      <w:p><w:r><w:t>被保留但不参与改写</w:t></w:r></w:p>
    </x:customBlock>
    <w:p><w:r><w:t>正文段落</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);

    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");

    assert!(regions.iter().any(|region| {
        region.body.contains("[复杂结构:customBlock]")
            && protect_kind_of(region) == Some("unknown-structure")
    }));
    assert!(regions
        .iter()
        .any(|region| region.body.contains("正文段落")));
}

#[test]
fn imports_unknown_run_structure_as_locked_placeholder_instead_of_rejecting() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
            xmlns:x="urn:lessai:unknown-run">
  <w:body>
    <w:p>
      <w:r><x:token/></w:r>
      <w:r><w:t>可编辑正文</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);

    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");

    assert!(regions.iter().any(|region| {
        region.body.contains("[复杂结构:token]")
            && protect_kind_of(region) == Some("unknown-structure")
    }));
    assert!(regions
        .iter()
        .any(|region| region.body.contains("可编辑正文")));
}

#[test]
fn rejects_unknown_article_object_that_cannot_be_classified_safely() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
            xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
            xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
            xmlns:x="urn:lessai:unknown-object">
  <w:body>
    <w:p>
      <w:r>
        <w:drawing>
          <wp:inline>
            <a:graphic>
              <a:graphicData uri="urn:lessai:unknown-object">
                <x:widget/>
              </a:graphicData>
            </a:graphic>
          </wp:inline>
        </w:drawing>
      </w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);

    let error = DocxAdapter::extract_regions(&bytes, false).expect_err("expected rejection");

    assert!(error.contains("图形对象") || error.contains("无法归类") || error.contains("不支持"));
}

#[test]
fn roundtrips_report_template_writeback_regions_with_locked_non_article_objects() {
    let bytes = load_repo_docx_fixture("04-3 作品报告（大数据应用赛，2025版）模板.docx");

    let source =
        DocxAdapter::extract_writeback_source_text(&bytes).expect("extract writeback source");
    let regions =
        DocxAdapter::extract_writeback_regions(&bytes).expect("extract writeback regions");

    assert_has_substantive_editable_article_regions(&regions);
    assert!(regions
        .iter()
        .any(|region| protect_kind_of(region) == Some("image")));
    assert!(regions
        .iter()
        .any(|region| protect_kind_of(region) == Some("textbox")));
    assert!(regions
        .iter()
        .any(|region| protect_kind_of(region) == Some("content-control")));
    assert!(regions
        .iter()
        .any(|region| protect_kind_of(region) == Some("table")));

    let rewritten = DocxAdapter::write_updated_regions(&bytes, &source, &regions)
        .expect("write updated regions");
    let rewritten_source = DocxAdapter::extract_writeback_source_text(&rewritten)
        .expect("extract rewritten writeback source");

    assert_eq!(rewritten_source, source);
}

#[test]
fn roundtrips_chart_shape_and_group_shape_placeholders_through_writeback() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
            xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
            xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
            xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
            xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"
            xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"
            xmlns:wpg="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup">
  <w:body>
    <w:p><w:r><w:t>正文</w:t></w:r></w:p>
    <w:p>
      <w:r>
        <w:drawing>
          <wp:inline>
            <a:graphic>
              <a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart">
                <c:chart r:id="rIdChart1"/>
              </a:graphicData>
            </a:graphic>
          </wp:inline>
        </w:drawing>
      </w:r>
    </w:p>
    <w:p>
      <w:r>
        <w:drawing>
          <wp:inline>
            <a:graphic>
              <a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">
                <wps:wsp/>
              </a:graphicData>
            </a:graphic>
          </wp:inline>
        </w:drawing>
      </w:r>
    </w:p>
    <w:p>
      <w:r>
        <w:drawing>
          <wp:inline>
            <a:graphic>
              <a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup">
                <wpg:wgp/>
              </a:graphicData>
            </a:graphic>
          </wp:inline>
        </w:drawing>
      </w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);

    let source = DocxAdapter::extract_writeback_source_text(&bytes).expect("extract source");
    let regions = DocxAdapter::extract_writeback_regions(&bytes).expect("extract regions");
    let rewritten =
        DocxAdapter::write_updated_regions(&bytes, &source, &regions).expect("write regions");

    assert_eq!(
        normalize_xml_layout(&read_docx_entry(&rewritten, "word/document.xml")),
        normalize_xml_layout(&read_docx_entry(&bytes, "word/document.xml"))
    );
}

#[test]
fn roundtrips_vml_pict_shape_placeholders_through_writeback() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
            xmlns:v="urn:schemas-microsoft-com:vml">
  <w:body>
    <w:p><w:r><w:t>正文</w:t></w:r></w:p>
    <w:p>
      <w:r>
        <w:pict>
          <v:rect style="width:0;height:1.5pt"/>
        </w:pict>
      </w:r>
    </w:p>
    <w:p>
      <w:r>
        <w:pict>
          <v:roundrect style="height:77.85pt;width:158.15pt"/>
        </w:pict>
      </w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);

    let source = DocxAdapter::extract_writeback_source_text(&bytes).expect("extract source");
    let regions = DocxAdapter::extract_writeback_regions(&bytes).expect("extract regions");
    let rewritten =
        DocxAdapter::write_updated_regions(&bytes, &source, &regions).expect("write regions");

    assert_eq!(
        normalize_xml_layout(&read_docx_entry(&rewritten, "word/document.xml")),
        normalize_xml_layout(&read_docx_entry(&bytes, "word/document.xml"))
    );
}

#[test]
fn preserves_chart_shape_or_group_placeholder_when_placeholder_text_changes() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
            xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
            xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
            xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
            xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart">
  <w:body>
    <w:p>
      <w:r>
        <w:drawing>
          <wp:inline>
            <a:graphic>
              <a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart">
                <c:chart r:id="rIdChart1"/>
              </a:graphicData>
            </a:graphic>
          </wp:inline>
        </w:drawing>
      </w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let source = DocxAdapter::extract_writeback_source_text(&bytes).expect("extract source");
    let mut regions = DocxAdapter::extract_writeback_regions(&bytes).expect("extract regions");
    let chart_region = regions
        .iter_mut()
        .find(|region| protect_kind_of(region) == Some("chart"))
        .expect("chart placeholder");
    chart_region.body = "[已改坏图表]".to_string();

    let rewritten = DocxAdapter::write_updated_regions(&bytes, &source, &regions)
        .expect("writeback should preserve locked placeholder");
    let extracted = DocxAdapter::extract_writeback_source_text(&rewritten).expect("extract source");

    assert_eq!(extracted, source);
    assert!(!extracted.contains("[已改坏图表]"));
}

#[test]
fn validates_writeback_for_simple_docx_with_list_paragraphs() {
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
    DocxAdapter::validate_writeback(&bytes).expect("writeback should be allowed");
}
