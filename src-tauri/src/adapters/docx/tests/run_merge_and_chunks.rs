use super::*;

#[test]
fn merges_adjacent_runs_when_only_rfonts_hint_differs() {
    let bytes = build_rfonts_hint_fragmented_docx();
    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");

    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].body, "2025年（第18届）");
    assert_single_editable_unit_for_all_presets(&bytes, "2025年（第18届）");
}

#[test]
fn writes_back_docx_when_runs_only_differ_by_rfonts_hint() {
    let bytes = build_rfonts_hint_fragmented_docx();
    let source = DocxAdapter::extract_text(&bytes).expect("extract text");
    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");

    let rewritten = DocxAdapter::write_updated_regions(&bytes, &source, &regions)
        .expect("write updated regions");
    let extracted = DocxAdapter::extract_text(&rewritten).expect("extract rewritten text");

    assert_eq!(extracted, source);
}

#[test]
fn merges_adjacent_runs_when_only_hint_only_rpr_differs_from_plain_text() {
    let bytes = build_hint_only_rpr_fragmented_docx();
    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");

    assert_eq!(joined_region_text(&regions), "Ctrl + 0");
    assert_single_editable_unit_for_all_presets(&bytes, "Ctrl + 0");
}

#[test]
fn merges_adjacent_runs_when_hint_only_rfonts_is_mixed_with_real_style_properties() {
    let bytes = build_color_and_hint_fragmented_docx();
    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");

    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].body, "建议不超过1页");
    assert_single_editable_unit_for_all_presets(&bytes, "建议不超过1页");
}

#[test]
fn merges_writeback_regions_when_hint_only_rfonts_would_otherwise_split_text() {
    let bytes = build_color_and_hint_fragmented_docx();
    let regions = DocxAdapter::extract_writeback_regions(&bytes).expect("extract writeback");

    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].body, "建议不超过1页");
    assert_single_editable_unit_for_all_presets(&bytes, "建议不超过1页");
}

#[test]
fn report_template_keeps_shortcuts_and_page_counts_in_whole_chunks() {
    let bytes = load_repo_docx_fixture("04-3 作品报告（大数据应用赛，2025版）模板.docx");
    let chunk_texts = editable_unit_texts(&bytes, SegmentationPreset::Clause);

    assert!(chunk_texts.iter().any(|text| text.contains("Ctrl + 0")));
    assert!(chunk_texts
        .iter()
        .any(|text| text.contains("建议控制在1页内") || text.contains("建议控制在 1 页内")));
}

#[test]
fn report_template_paragraph_chunks_do_not_expose_empty_editable_chunks() {
    let bytes = load_repo_docx_fixture("04-3 作品报告（大数据应用赛，2025版）模板.docx");
    let chunks = editable_unit_texts(&bytes, SegmentationPreset::Paragraph);

    assert!(
        !chunks.iter().any(|chunk| chunk.trim().is_empty()),
        "expected no editable blank chunks, got:\n{:?}",
        chunks
    );
}

#[test]
fn report_template_paragraph_chunks_keep_manual_line_break_samples_together() {
    let bytes = load_repo_docx_fixture("04-3 作品报告（大数据应用赛，2025版）模板.docx");
    let chunks = editable_unit_texts(&bytes, SegmentationPreset::Paragraph);

    assert!(
        chunks.iter().any(|chunk| {
            chunk.contains("以下为数据样例：")
                && chunk.contains("样例1（表格数据）：")
                && chunk.contains("001, 2024-01-15, 类型1, 23.5, 87.2, 正常")
                && !chunk.contains("样例2（JSON数据）：")
        }),
        "expected sample intro and csv lines to stay in one paragraph chunk, got:\n{:?}",
        chunks
    );
    assert!(
        chunks.iter().any(|chunk| {
            chunk.contains("样例2（JSON数据）：")
                && chunk.contains("time: 2024-01-17 10:23:15")
                && chunk.contains("label: 正常")
        }),
        "expected json sample lines to stay in one paragraph chunk, got:\n{:?}",
        chunks
    );
}

#[test]
fn report_template_cover_image_unit_does_not_absorb_extra_empty_paragraph_breaks() {
    let bytes = load_repo_docx_fixture("04-3 作品报告（大数据应用赛，2025版）模板.docx");
    let units = rewrite_unit_texts(&bytes, SegmentationPreset::Paragraph);
    let image_unit = units
        .iter()
        .find(|text| text.contains("[图片]"))
        .expect("expected image placeholder unit");

    assert!(
        !image_unit.contains("\n\n\n\n"),
        "expected cover image unit to avoid compounded blank paragraphs, got:\n{:?}\nfirst units:\n{:?}",
        image_unit,
        units.iter().take(12).collect::<Vec<_>>()
    );
}

#[test]
fn report_template_cover_units_do_not_emit_compounded_blank_gaps() {
    let bytes = load_repo_docx_fixture("04-3 作品报告（大数据应用赛，2025版）模板.docx");
    let units = rewrite_unit_texts(&bytes, SegmentationPreset::Paragraph);
    let cover_units = units.iter().take(12).collect::<Vec<_>>();

    assert!(
        !cover_units.iter().any(|text| text.contains("\n\n\n\n")),
        "expected cover units to avoid compounded blank gaps, got:\n{:?}",
        cover_units
    );
}

#[test]
fn report_template_paragraph_units_do_not_include_blank_only_locked_units() {
    let bytes = load_repo_docx_fixture("04-3 作品报告（大数据应用赛，2025版）模板.docx");
    let units = rewrite_unit_texts(&bytes, SegmentationPreset::Paragraph);
    let blank_only = units
        .iter()
        .filter(|text| !text.is_empty() && text.trim().is_empty())
        .collect::<Vec<_>>();

    assert!(
        blank_only.is_empty(),
        "expected no blank-only paragraph units, got:\n{:?}",
        blank_only
    );
}

#[test]
fn keeps_distinct_regions_for_runs_with_different_raw_properties() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r>
        <w:rPr><w:lang w:val="en-US"/></w:rPr>
        <w:t>A</w:t>
      </w:r>
      <w:r><w:t>B</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
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

