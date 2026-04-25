use super::*;

#[test]
fn extracts_plain_text_from_docx_document_xml() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>第一段</w:t></w:r></w:p>
    <w:p><w:r><w:t>第二段</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let text = DocxAdapter::extract_text(&bytes).expect("extract text");
    assert_eq!(text, "第一段\n\n第二段");
}

#[test]
fn allows_body_level_bookmark_markers_without_rejecting_docx() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:bookmarkStart w:id="0" w:name="_GoBack"/>
    <w:p><w:r><w:t>正文段落</w:t></w:r></w:p>
    <w:bookmarkEnd w:id="0"/>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let source = DocxAdapter::extract_text(&bytes).expect("extract text");
    let writeback_source =
        DocxAdapter::extract_writeback_source_text(&bytes).expect("extract writeback source");
    let rewritten =
        DocxAdapter::write_updated_text(&bytes, &source, &source).expect("write updated text");
    let rewritten_text = DocxAdapter::extract_text(&rewritten).expect("extract rewritten text");
    let rewritten_document_xml = read_docx_entry(&rewritten, "word/document.xml");

    assert_eq!(source, "正文段落");
    assert_eq!(writeback_source, source);
    assert_eq!(rewritten_text, source);
    assert!(rewritten_document_xml.contains("<w:bookmarkStart"));
    assert!(rewritten_document_xml.contains("<w:bookmarkEnd"));
}

#[test]
fn imports_tabs_as_visible_text_during_import() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:t>a</w:t></w:r>
      <w:r><w:tab/></w:r>
      <w:r><w:t>b</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let text = DocxAdapter::extract_text(&bytes).expect("extract text");
    assert_eq!(text, "a\tb");
}

#[test]
fn imports_line_breaks_as_visible_newlines_during_import() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:t>a</w:t></w:r>
      <w:r><w:br/></w:r>
      <w:r><w:t>b</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let text = DocxAdapter::extract_text(&bytes).expect("extract text");
    assert_eq!(text, "a\nb");
}

#[test]
fn extract_writeback_slots_split_manual_line_breaks_into_separate_slots() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:t>甲</w:t><w:br/><w:t>乙</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);

    let slots = DocxAdapter::extract_writeback_slots(&bytes, false).expect("extract slots");

    assert_eq!(slots.len(), 2);
    assert_eq!(slots[0].text, "甲");
    assert_eq!(slots[0].separator_after, "\n");
    assert_eq!(slots[1].text, "乙");
    assert_eq!(slots[1].separator_after, "");
}

#[test]
fn paragraph_preset_keeps_manual_line_breaks_inside_same_editable_unit() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:t>第一行</w:t><w:br/><w:t>第二行</w:t><w:br/><w:t>第三行</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);

    let editable_units = editable_unit_texts(&bytes, SegmentationPreset::Paragraph);

    assert_eq!(editable_units, vec!["第一行\n第二行\n第三行".to_string()]);
}

#[test]
fn imports_carriage_returns_as_visible_newlines_during_import() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:t>a</w:t></w:r>
      <w:r><w:cr/></w:r>
      <w:r><w:t>b</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let text = DocxAdapter::extract_text(&bytes).expect("extract text");
    assert_eq!(text, "a\nb");
}

#[test]
fn keeps_empty_paragraphs_as_blank_lines() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p></w:p>
    <w:p><w:r><w:t>正文</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let text = DocxAdapter::extract_text(&bytes).expect("extract text");
    assert_eq!(text, "\n\n正文");
}

#[test]
fn imports_empty_paragraphs_as_locked_separators() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p></w:p>
    <w:p><w:r><w:t>正文</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);

    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");

    assert_eq!(
        regions.first().map(|region| region.body.as_str()),
        Some("\n\n")
    );
    assert!(regions.first().is_some_and(|region| region.skip_rewrite));
    assert!(regions
        .first()
        .and_then(|region| region.presentation.as_ref())
        .is_none());
}

#[test]
fn extracts_list_item_text_from_docx() {
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
    let text = DocxAdapter::extract_text(&bytes).expect("extract text");
    assert_eq!(text, "第一项");
}

#[test]
fn marks_heading_styles_as_skip_regions_by_default() {
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

    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");
    assert!(regions
        .iter()
        .any(|region| region.skip_rewrite && region.body.contains("标题")));

    let rebuilt = regions
        .iter()
        .map(|region| region.body.as_str())
        .collect::<String>();
    let text = DocxAdapter::extract_text(&bytes).expect("extract text");
    assert_eq!(rebuilt, text);
}

#[test]
fn allows_heading_styles_to_be_rewritten_when_enabled() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:pPr><w:pStyle w:val="Title"/></w:pPr>
      <w:r><w:t>文档标题</w:t></w:r>
    </w:p>
    <w:p><w:r><w:t>正文</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);

    let regions = DocxAdapter::extract_regions(&bytes, true).expect("extract regions");
    assert!(regions
        .iter()
        .any(|region| !region.skip_rewrite && region.body.contains("文档标题")));

    let rebuilt = regions
        .iter()
        .map(|region| region.body.as_str())
        .collect::<String>();
    let text = DocxAdapter::extract_text(&bytes).expect("extract text");
    assert_eq!(rebuilt, text);
}

#[test]
fn imports_softwrapped_line_wrapped_docx_during_import() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>这一段被硬换行拆成很多行</w:t></w:r></w:p>
    <w:p><w:r><w:t>每行都成了一个段落导致切块过碎</w:t></w:r></w:p>
    <w:p><w:r><w:t>导入时需要做轻量合并</w:t></w:r></w:p>
    <w:p><w:r><w:t>否则连一句完整的话都不在同一块里</w:t></w:r></w:p>
    <w:p><w:r><w:t>这里继续补一些行以触发启发式</w:t></w:r></w:p>
    <w:p><w:r><w:t>第六行内容用于模拟真实文档</w:t></w:r></w:p>
    <w:p><w:r><w:t>第七行内容用于模拟真实文档</w:t></w:r></w:p>
    <w:p><w:r><w:t>第八行内容用于模拟真实文档</w:t></w:r></w:p>
    <w:p><w:r><w:t>第九行内容用于模拟真实文档</w:t></w:r></w:p>
    <w:p><w:r><w:t>第十行内容用于模拟真实文档</w:t></w:r></w:p>
    <w:p><w:r><w:t>第十一行内容用于模拟真实文档</w:t></w:r></w:p>
    <w:p><w:r><w:t>最后一行收尾。</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let text = DocxAdapter::extract_text(&bytes).expect("extract text");
    assert!(text.contains("这一段被硬换行拆成很多行"));
    assert!(text.contains("最后一行收尾。"));
}

#[test]
fn imports_softwrapped_line_wrapped_docx_with_fewer_lines_during_import() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>这一段被硬换行拆成很多行</w:t></w:r></w:p>
    <w:p><w:r><w:t>每行都成了一个段落导致切块过碎</w:t></w:r></w:p>
    <w:p><w:r><w:t>导入时需要做轻量合并</w:t></w:r></w:p>
    <w:p><w:r><w:t>否则连一句完整的话都不在同一块里</w:t></w:r></w:p>
    <w:p><w:r><w:t>这里继续补一些行以触发启发式</w:t></w:r></w:p>
    <w:p><w:r><w:t>第六行内容用于模拟真实文档</w:t></w:r></w:p>
    <w:p><w:r><w:t>第七行内容用于模拟真实文档</w:t></w:r></w:p>
    <w:p><w:r><w:t>第八行内容用于模拟真实文档</w:t></w:r></w:p>
    <w:p><w:r><w:t>第九行内容用于模拟真实文档</w:t></w:r></w:p>
    <w:p><w:r><w:t>最后一行收尾。</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let text = DocxAdapter::extract_text(&bytes).expect("extract text");
    assert!(text.contains("每行都成了一个段落导致切块过碎"));
    assert!(text.contains("最后一行收尾。"));
}

#[test]
fn allows_writeback_for_softwrapped_line_wrapped_docx() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>这一段被硬换行拆成很多行</w:t></w:r></w:p>
    <w:p><w:r><w:t>每行都成了一个段落导致切块过碎</w:t></w:r></w:p>
    <w:p><w:r><w:t>导入时需要做轻量合并</w:t></w:r></w:p>
    <w:p><w:r><w:t>否则连一句完整的话都不在同一块里</w:t></w:r></w:p>
    <w:p><w:r><w:t>这里继续补一些行以触发启发式</w:t></w:r></w:p>
    <w:p><w:r><w:t>第六行内容用于模拟真实文档</w:t></w:r></w:p>
    <w:p><w:r><w:t>第七行内容用于模拟真实文档</w:t></w:r></w:p>
    <w:p><w:r><w:t>第八行内容用于模拟真实文档</w:t></w:r></w:p>
    <w:p><w:r><w:t>第九行内容用于模拟真实文档</w:t></w:r></w:p>
    <w:p><w:r><w:t>第十行内容用于模拟真实文档</w:t></w:r></w:p>
    <w:p><w:r><w:t>第十一行内容用于模拟真实文档</w:t></w:r></w:p>
    <w:p><w:r><w:t>最后一行收尾。</w:t></w:r></w:p>
    </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let source = DocxAdapter::extract_text(&bytes).expect("extract text");
    let rewritten =
        DocxAdapter::write_updated_text(&bytes, &source, &source).expect("expected success");
    let extracted = DocxAdapter::extract_text(&rewritten).expect("extract rewritten text");
    assert_eq!(extracted, source);
}

#[test]
fn imports_report_template_with_locked_non_article_objects() {
    let bytes = load_repo_docx_fixture("04-3 作品报告（大数据应用赛，2025版）模板.docx");

    let regions = DocxAdapter::extract_regions(&bytes, false).expect("import template");

    assert_has_substantive_editable_article_regions(&regions);
    assert!(!regions
        .iter()
        .any(|region| !region.skip_rewrite && region.body.trim().is_empty()));
    assert!(regions
        .iter()
        .any(|region| protect_kind_of(region) == Some("image")));
    assert!(regions
        .iter()
        .any(|region| protect_kind_of(region) == Some("textbox")));
    assert!(regions
        .iter()
        .any(|region| protect_kind_of(region) == Some("table")));
}

#[test]
fn does_not_lock_regular_body_paragraphs_just_because_text_mentions_instruction_words() {
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:pPr><w:pStyle w:val="1"/></w:pPr>
      <w:r><w:t>第1章 系统背景</w:t></w:r>
    </w:p>
    <w:p>
      <w:pPr><w:pStyle w:val="a0"/></w:pPr>
      <w:r><w:t>系统建议不超过 5 秒完成重试，但这只是正文里的性能约束描述，请勿修改其业务含义。</w:t></w:r>
    </w:p>
    <w:p>
      <w:pPr><w:pStyle w:val="a0"/></w:pPr>
      <w:r><w:t>这是一段正常的正文说明，用于补充背景、目标和约束条件，确保系统能够正确识别文章主体内容。</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let styles_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:style w:type="paragraph" w:styleId="1">
    <w:name w:val="heading 1"/>
  </w:style>
  <w:style w:type="paragraph" w:styleId="a0">
    <w:name w:val="正文段落"/>
  </w:style>
</w:styles>"#;
    let bytes = build_docx_entries(&[
        ("word/document.xml", document_xml),
        ("word/styles.xml", styles_xml),
    ]);

    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");

    assert_region_with_text_editable(
        &regions,
        "系统建议不超过 5 秒完成重试，但这只是正文里的性能约束描述，请勿修改其业务含义。",
    );
}

#[test]
fn report_template_keeps_first_heading_numbered_as_chapter_one() {
    let bytes = load_repo_docx_fixture("04-3 作品报告（大数据应用赛，2025版）模板.docx");

    let text = DocxAdapter::extract_text(&bytes).expect("extract text");
    let source = DocxAdapter::extract_writeback_source_text(&bytes).expect("extract source");
    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");
    let rebuilt = joined_region_text(&regions);

    assert!(
        text.contains("作品概述"),
        "expected heading text, got:\n{text}"
    );
    assert!(
        source.contains("作品概述"),
        "expected heading text in source, got:\n{source}"
    );
    assert!(
        rebuilt.contains("作品概述"),
        "expected heading text in regions, got:\n{rebuilt}"
    );
    assert!(
        text.contains("第1章 ") || text.contains("1 "),
        "expected numbered heading marker, got:\n{text}"
    );
}

#[test]
fn imports_underlined_blank_runs_as_locked_underlined_text() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:t>填写日期：</w:t></w:r>
      <w:r>
        <w:rPr><w:u w:val="single"/></w:rPr>
        <w:t xml:space="preserve">　　　　</w:t>
      </w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);

    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");
    let rebuilt = joined_region_text(&regions);
    let blank_region = regions
        .iter()
        .find(|region| region.body == "　　　　")
        .expect("underlined blank region");
    let presentation = blank_region
        .presentation
        .as_ref()
        .expect("underlined blank presentation");

    assert_eq!(rebuilt, "填写日期：　　　　");
    assert!(blank_region.skip_rewrite);
    assert!(presentation.underline);
    assert_eq!(presentation.protect_kind.as_deref(), None);
}

#[test]
fn splits_underlined_run_edge_whitespace_into_locked_regions() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:t>作品编号：</w:t></w:r>
      <w:r>
        <w:rPr><w:u w:val="single"/></w:rPr>
        <w:t xml:space="preserve">　　ABC123　　　</w:t>
      </w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);

    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");
    let rebuilt = joined_region_text(&regions);
    let underlined_regions = regions
        .iter()
        .filter(|region| {
            region
                .presentation
                .as_ref()
                .is_some_and(|presentation| presentation.underline)
        })
        .map(|region| (region.body.as_str(), region.skip_rewrite))
        .collect::<Vec<_>>();

    assert_eq!(rebuilt, "作品编号：　　ABC123　　　");
    assert_eq!(
        underlined_regions,
        vec![("　　", true), ("ABC123", false), ("　　　", true)]
    );
}

#[test]
fn writes_back_underlined_run_with_locked_edge_whitespace() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:t>作品编号：</w:t></w:r>
      <w:r>
        <w:rPr><w:u w:val="single"/></w:rPr>
        <w:t xml:space="preserve">　　ABC123　　　</w:t>
      </w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let source = DocxAdapter::extract_writeback_source_text(&bytes).expect("extract source");
    let mut regions = DocxAdapter::extract_writeback_regions(&bytes).expect("extract regions");
    let editable_region = regions
        .iter_mut()
        .find(|region| !region.skip_rewrite && region.body == "ABC123")
        .expect("editable fill content");
    editable_region.body = "ZX-9".to_string();

    let rewritten = DocxAdapter::write_updated_regions(&bytes, &source, &regions)
        .expect("write updated regions");
    let extracted = DocxAdapter::extract_writeback_regions(&rewritten).expect("extract rewritten");
    let underlined_regions = extracted
        .iter()
        .filter(|region| {
            region
                .presentation
                .as_ref()
                .is_some_and(|presentation| presentation.underline)
        })
        .map(|region| (region.body.as_str(), region.skip_rewrite))
        .collect::<Vec<_>>();

    assert_eq!(
        underlined_regions,
        vec![("　　", true), ("ZX-9", false), ("　　　", true)]
    );
}

