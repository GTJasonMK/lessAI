use super::DocxAdapter;
use crate::{
    adapters::TextRegion,
    models::{SegmentationPreset, TextPresentation},
    rewrite_unit::build_rewrite_units,
    test_support::{
        build_chunk_test_fixture_docx, build_docx_entries, build_minimal_docx,
        build_report_template_fixture_docx, load_repo_docx_fixture_or,
    },
};
use zip::ZipArchive;

fn read_docx_entry(bytes: &[u8], name: &str) -> String {
    let cursor = std::io::Cursor::new(bytes);
    let mut zip = ZipArchive::new(cursor).expect("open zip");
    let mut file = zip.by_name(name).expect("open entry");
    let mut out = String::new();
    use std::io::Read;
    file.read_to_string(&mut out).expect("read entry");
    out
}

fn load_repo_docx_fixture(file_name: &str) -> Vec<u8> {
    load_repo_docx_fixture_or(file_name, build_report_template_fixture_docx)
}

fn protect_kind_of(region: &TextRegion) -> Option<&str> {
    region
        .presentation
        .as_ref()
        .and_then(|item| item.protect_kind.as_deref())
}

fn joined_region_text(regions: &[TextRegion]) -> String {
    regions.iter().map(|region| region.body.as_str()).collect()
}

fn assert_region_with_text_editable(regions: &[TextRegion], needle: &str) {
    assert!(
        regions
            .iter()
            .any(|region| !region.skip_rewrite && region.body.contains(needle)),
        "expected editable region containing `{needle}`, got:\n{}",
        joined_region_text(regions)
    );
}

fn assert_has_substantive_editable_article_regions(regions: &[TextRegion]) {
    let editable_regions = regions
        .iter()
        .filter(|region| !region.skip_rewrite)
        .collect::<Vec<_>>();
    assert!(
        editable_regions.len() >= 10,
        "expected many editable regions in report-like fixture, got {}:\n{}",
        editable_regions.len(),
        joined_region_text(regions)
    );
    assert!(
        editable_regions.iter().any(|region| {
            region.presentation.is_none()
                && region.body.chars().count() >= 8
                && region.body.chars().any(|ch| !ch.is_whitespace())
        }),
        "expected substantive editable article text, got:\n{}",
        joined_region_text(regions)
    );
}

fn normalize_xml_layout(xml: &str) -> String {
    xml.lines().map(str::trim).collect::<String>()
}

fn build_rfonts_hint_fragmented_docx() -> Vec<u8> {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r>
        <w:rPr>
          <w:rFonts w:ascii="华文中宋" w:eastAsia="华文中宋" w:hAnsi="华文中宋" w:cs="Arial" w:hint="eastAsia"/>
          <w:b/><w:bCs/><w:color w:val="333333"/><w:kern w:val="0"/><w:sz w:val="56"/><w:szCs w:val="56"/>
        </w:rPr>
        <w:t>2</w:t>
      </w:r>
      <w:r>
        <w:rPr>
          <w:rFonts w:ascii="华文中宋" w:eastAsia="华文中宋" w:hAnsi="华文中宋" w:cs="Arial"/>
          <w:b/><w:bCs/><w:color w:val="333333"/><w:kern w:val="0"/><w:sz w:val="56"/><w:szCs w:val="56"/>
        </w:rPr>
        <w:t>02</w:t>
      </w:r>
      <w:r>
        <w:rPr>
          <w:rFonts w:ascii="华文中宋" w:eastAsia="华文中宋" w:hAnsi="华文中宋" w:cs="Arial" w:hint="eastAsia"/>
          <w:b/><w:bCs/><w:color w:val="333333"/><w:kern w:val="0"/><w:sz w:val="56"/><w:szCs w:val="56"/>
        </w:rPr>
        <w:t>5年（第18届）</w:t>
      </w:r>
    </w:p>
  </w:body>
</w:document>"#;
    build_minimal_docx(xml)
}

fn build_hint_only_rpr_fragmented_docx() -> Vec<u8> {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:rPr><w:rFonts w:hint="eastAsia"/></w:rPr><w:t>Ctrl</w:t></w:r>
      <w:r><w:t xml:space="preserve"> </w:t></w:r>
      <w:r><w:rPr><w:rFonts w:hint="eastAsia"/></w:rPr><w:t>+</w:t></w:r>
      <w:r><w:t xml:space="preserve"> 0</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    build_minimal_docx(xml)
}

fn build_color_and_hint_fragmented_docx() -> Vec<u8> {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:rPr><w:color w:val="FF0000"/></w:rPr><w:t>建议不超过</w:t></w:r>
      <w:r><w:rPr><w:color w:val="FF0000"/></w:rPr><w:t>1</w:t></w:r>
      <w:r><w:rPr><w:rFonts w:hint="eastAsia"/><w:color w:val="FF0000"/></w:rPr><w:t>页</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    build_minimal_docx(xml)
}

fn editable_unit_texts(bytes: &[u8], preset: SegmentationPreset) -> Vec<String> {
    let slots = DocxAdapter::extract_writeback_slots(bytes, false).expect("extract slots");
    build_rewrite_units(&slots, preset)
        .into_iter()
        .filter(|unit| {
            unit.slot_ids.iter().any(|slot_id| {
                slots
                    .iter()
                    .any(|slot| slot.id == *slot_id && slot.editable)
            })
        })
        .map(|unit| unit.display_text)
        .collect()
}

fn rewrite_unit_texts(bytes: &[u8], preset: SegmentationPreset) -> Vec<String> {
    let slots = DocxAdapter::extract_writeback_slots(bytes, false).expect("extract slots");
    build_rewrite_units(&slots, preset)
        .into_iter()
        .map(|unit| unit.display_text)
        .collect()
}

fn assert_single_editable_unit_for_all_presets(bytes: &[u8], expected: &str) {
    for preset in [
        SegmentationPreset::Clause,
        SegmentationPreset::Sentence,
        SegmentationPreset::Paragraph,
    ] {
        let editable_units = editable_unit_texts(bytes, preset);
        assert_eq!(editable_units.len(), 1);
        assert_eq!(editable_units[0], expected);
    }
}


#[path = "tests/import_basic.rs"]
mod import_basic;
#[path = "tests/numbering.rs"]
mod numbering;
#[path = "tests/placeholders_roundtrip.rs"]
mod placeholders_roundtrip;
#[path = "tests/hyperlink_formula.rs"]
mod hyperlink_formula;
#[path = "tests/editor_writeback.rs"]
mod editor_writeback;
#[path = "tests/placeholders_misc.rs"]
mod placeholders_misc;
#[path = "tests/run_merge_and_chunks.rs"]
mod run_merge_and_chunks;
#[path = "tests/hyperlink_locked.rs"]
mod hyperlink_locked;
#[path = "tests/regression_misc.rs"]
mod regression_misc;
