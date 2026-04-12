use super::DocxAdapter;
use crate::{
    adapters::TextRegion,
    documents::RegionSegmentationStrategy,
    models::{
        ChunkPresentation, ChunkPreset, ChunkStatus, ChunkTask, DocumentFormat, DocumentSession,
        RunningState,
    },
    rewrite, rewrite_jobs,
};
use chrono::Utc;
use std::{fs, io::Write, path::PathBuf};
use zip::{write::FileOptions, ZipArchive, ZipWriter};

fn build_minimal_docx(document_xml: &str) -> Vec<u8> {
    build_docx_entries(&[("word/document.xml", document_xml)])
}

fn build_docx_entries(entries: &[(&str, &str)]) -> Vec<u8> {
    let mut out = Vec::new();
    let cursor = std::io::Cursor::new(&mut out);
    let mut zip = ZipWriter::new(cursor);
    let options = FileOptions::<()>::default();

    for (name, contents) in entries {
        zip.start_file(*name, options).expect("start file");
        zip.write_all(contents.as_bytes()).expect("write xml");
    }
    zip.finish().expect("finish zip");
    out
}

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
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("testdoc")
        .join(file_name);
    fs::read(path).expect("read docx fixture")
}

fn protect_kind_of(region: &TextRegion) -> Option<&str> {
    region
        .presentation
        .as_ref()
        .and_then(|item| item.protect_kind.as_deref())
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

fn assert_single_editable_chunk_for_all_presets(regions: &[TextRegion], expected: &str) {
    for preset in [
        ChunkPreset::Clause,
        ChunkPreset::Sentence,
        ChunkPreset::Paragraph,
    ] {
        let chunks = rewrite::segment_regions_with_strategy(
            regions.to_vec(),
            preset,
            DocumentFormat::PlainText,
            RegionSegmentationStrategy::PreserveBoundaries,
        );
        let editable_chunks: Vec<&crate::rewrite::SegmentedChunk> =
            chunks.iter().filter(|chunk| !chunk.skip_rewrite).collect();
        assert_eq!(editable_chunks.len(), 1);
        assert_eq!(editable_chunks[0].text, expected);
    }
}

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
fn rejects_writeback_for_softwrapped_line_wrapped_docx() {
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
    let error =
        DocxAdapter::write_updated_text(&bytes, &source, &source).expect_err("expected failure");
    assert!(error.contains("不支持") || error.contains("仅支持简单 docx"));
    assert!(error.contains("按行断开") || error.contains("简单"));
}

#[test]
fn imports_report_template_with_locked_non_article_objects() {
    let bytes = load_repo_docx_fixture("04-3 作品报告（大数据应用赛，2025版）模板.docx");

    let regions = DocxAdapter::extract_regions(&bytes, false).expect("import template");

    assert!(regions
        .iter()
        .any(|region| !region.skip_rewrite && region.body.contains("作品名称")));
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
        .any(|region| region.body == "[图表]" && protect_kind_of(region) == Some("chart")));
    assert!(regions
        .iter()
        .any(|region| region.body == "[图形]" && protect_kind_of(region) == Some("shape")));
    assert!(regions.iter().any(|region| {
        region.body == "[组合图形]" && protect_kind_of(region) == Some("group-shape")
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

    assert!(regions
        .iter()
        .any(|region| !region.skip_rewrite && region.body.contains("作品名称")));
    assert!(regions
        .iter()
        .any(|region| protect_kind_of(region) == Some("image")));
    assert!(regions
        .iter()
        .any(|region| protect_kind_of(region) == Some("textbox")));
    assert!(regions
        .iter()
        .any(|region| protect_kind_of(region) == Some("toc")));
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
        read_docx_entry(&rewritten, "word/document.xml"),
        read_docx_entry(&bytes, "word/document.xml")
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
        read_docx_entry(&rewritten, "word/document.xml"),
        read_docx_entry(&bytes, "word/document.xml")
    );
}

#[test]
fn rejects_writeback_when_chart_shape_or_group_placeholder_text_changes() {
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

    let error = DocxAdapter::write_updated_regions(&bytes, &source, &regions)
        .expect_err("reject writeback");

    assert!(error.contains("锁定内容") || error.contains("占位符") || error.contains("锁定区"));
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
        TextRegion {
            body: "访问".to_string(),
            skip_rewrite: false,
            presentation: None,
        },
        TextRegion {
            body: "新版链接".to_string(),
            skip_rewrite: false,
            presentation: Some(hyperlink_presentation),
        },
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
    let updated_regions = vec![TextRegion {
        body: "更新样式文本".to_string(),
        skip_rewrite: false,
        presentation: Some(presentation),
    }];

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
        TextRegion {
            body: "更新正文".to_string(),
            skip_rewrite: false,
            presentation: None,
        },
        TextRegion {
            body: "E=mc^2".to_string(),
            skip_rewrite: true,
            presentation: Some(ChunkPresentation {
                bold: false,
                italic: false,
                underline: false,
                href: None,
                protect_kind: Some("formula".to_string()),
                writeback_key: None,
            }),
        },
        TextRegion {
            body: "更新结论".to_string(),
            skip_rewrite: false,
            presentation: None,
        },
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
    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");

    for preset in [
        ChunkPreset::Clause,
        ChunkPreset::Sentence,
        ChunkPreset::Paragraph,
    ] {
        let chunks = rewrite::segment_regions_with_strategy(
            regions.clone(),
            preset,
            DocumentFormat::PlainText,
            RegionSegmentationStrategy::PreserveBoundaries,
        )
        .into_iter()
        .enumerate()
        .map(|(index, chunk)| ChunkTask {
            index,
            source_text: chunk.text,
            separator_after: chunk.separator_after,
            skip_rewrite: chunk.skip_rewrite,
            presentation: chunk.presentation,
            status: if chunk.skip_rewrite {
                ChunkStatus::Done
            } else {
                ChunkStatus::Idle
            },
            error_message: None,
        })
        .collect::<Vec<_>>();
        let now = Utc::now();
        let session = DocumentSession {
            id: format!("adjacent-formula-{preset:?}"),
            title: "test".to_string(),
            document_path: "test.docx".to_string(),
            source_text: source.clone(),
            source_snapshot: None,
            normalized_text: source.clone(),
            write_back_supported: true,
            write_back_block_reason: None,
            plain_text_editor_safe: false,
            plain_text_editor_block_reason: None,
            chunk_preset: Some(crate::models::ChunkPreset::Paragraph),
            rewrite_headings: Some(false),
            chunks,
            suggestions: Vec::new(),
            next_suggestion_sequence: 1,
            status: RunningState::Idle,
            created_at: now,
            updated_at: now,
        };

        let merged = rewrite_jobs::build_merged_regions(&session);
        let rewritten = DocxAdapter::write_updated_regions(&bytes, &source, &merged)
            .expect("write updated regions after chunk roundtrip");
        let extracted = DocxAdapter::extract_writeback_source_text(&rewritten)
            .expect("extract rewritten writeback source");

        assert_eq!(extracted, source);
    }
}

#[test]
fn rejects_region_writeback_when_locked_formula_text_changes() {
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
        TextRegion {
            body: "正文".to_string(),
            skip_rewrite: false,
            presentation: None,
        },
        TextRegion {
            body: "被改坏的公式".to_string(),
            skip_rewrite: true,
            presentation: Some(ChunkPresentation {
                bold: false,
                italic: false,
                underline: false,
                href: None,
                protect_kind: Some("formula".to_string()),
                writeback_key: None,
            }),
        },
    ];

    let error = DocxAdapter::write_updated_regions(&bytes, &source, &updated_regions)
        .expect_err("expected locked formula failure");
    assert!(error.contains("锁定") || error.contains("公式") || error.contains("占位"));
}

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
fn validates_plain_text_editor_for_docx_with_single_styled_region_per_paragraph() {
    let bytes = build_rfonts_hint_fragmented_docx();

    DocxAdapter::validate_plain_text_editor(&bytes).expect("plain text editor should be allowed");
}

#[test]
fn validates_plain_text_editor_for_docx_with_multiple_editable_regions() {
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

    DocxAdapter::validate_plain_text_editor(&bytes).expect("plain text editor should be allowed");
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
fn rejects_updated_text_for_docx_when_edit_crosses_style_boundary() {
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

    let error = DocxAdapter::write_updated_text(&bytes, &source, "加X文")
        .expect_err("expected style-boundary validation failure");

    assert!(error.contains("样式边界") || error.contains("安全写回") || error.contains("锁定内容"));
}

#[test]
fn validates_plain_text_editor_for_report_template() {
    let bytes = load_repo_docx_fixture("04-3 作品报告（大数据应用赛，2025版）模板.docx");

    DocxAdapter::validate_plain_text_editor(&bytes).expect("plain text editor should be allowed");
}

#[test]
fn validates_plain_text_editor_for_docx_with_inline_locked_formula() {
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

    DocxAdapter::validate_plain_text_editor(&bytes).expect("plain text editor should be allowed");
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
fn rejects_updated_text_when_inline_locked_formula_changes() {
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
        .expect_err("expected validation failure");

    assert!(error.contains("锁定") || error.contains("公式") || error.contains("占位"));
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
    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");

    for preset in [
        ChunkPreset::Clause,
        ChunkPreset::Sentence,
        ChunkPreset::Paragraph,
    ] {
        let chunks = rewrite::segment_regions_with_strategy(
            regions.clone(),
            preset,
            DocumentFormat::PlainText,
            RegionSegmentationStrategy::PreserveBoundaries,
        )
        .into_iter()
        .enumerate()
        .map(|(index, chunk)| ChunkTask {
            index,
            source_text: chunk.text,
            separator_after: chunk.separator_after,
            skip_rewrite: chunk.skip_rewrite,
            presentation: chunk.presentation,
            status: if chunk.skip_rewrite {
                ChunkStatus::Done
            } else {
                ChunkStatus::Idle
            },
            error_message: None,
        })
        .collect::<Vec<_>>();
        let now = Utc::now();
        let session = DocumentSession {
            id: "test".to_string(),
            title: "test".to_string(),
            document_path: "test.docx".to_string(),
            source_text: source.clone(),
            source_snapshot: None,
            normalized_text: source.clone(),
            write_back_supported: true,
            write_back_block_reason: None,
            plain_text_editor_safe: false,
            plain_text_editor_block_reason: None,
            chunk_preset: Some(crate::models::ChunkPreset::Paragraph),
            rewrite_headings: Some(false),
            chunks,
            suggestions: Vec::new(),
            next_suggestion_sequence: 1,
            status: RunningState::Idle,
            created_at: now,
            updated_at: now,
        };

        let merged = rewrite_jobs::build_merged_regions(&session);
        let rewritten = DocxAdapter::write_updated_regions(&bytes, &source, &merged)
            .expect("write updated regions after chunk roundtrip");
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
    let now = Utc::now();
    let session = DocumentSession {
        id: "collapsed".to_string(),
        title: "collapsed".to_string(),
        document_path: "collapsed.docx".to_string(),
        source_text: source.clone(),
        source_snapshot: None,
        normalized_text: source.clone(),
        write_back_supported: true,
        write_back_block_reason: None,
        plain_text_editor_safe: false,
        plain_text_editor_block_reason: None,
        chunk_preset: Some(crate::models::ChunkPreset::Paragraph),
        rewrite_headings: Some(false),
        chunks: vec![
            ChunkTask {
                index: 0,
                source_text: "第一段".to_string(),
                separator_after: "\n\n\n\n".to_string(),
                skip_rewrite: false,
                presentation: None,
                status: ChunkStatus::Idle,
                error_message: None,
            },
            ChunkTask {
                index: 1,
                source_text: "第二段".to_string(),
                separator_after: String::new(),
                skip_rewrite: false,
                presentation: None,
                status: ChunkStatus::Idle,
                error_message: None,
            },
        ],
        suggestions: Vec::new(),
        next_suggestion_sequence: 1,
        status: RunningState::Idle,
        created_at: now,
        updated_at: now,
    };

    let merged = rewrite_jobs::build_merged_regions(&session);
    let rewritten = DocxAdapter::write_updated_regions(&bytes, &source, &merged)
        .expect("write updated regions from collapsed separators");
    let extracted = DocxAdapter::extract_text(&rewritten).expect("extract rewritten text");

    assert_eq!(extracted, source);
}

#[test]
fn writes_back_docx_with_locked_heading_regions() {
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

#[test]
fn merges_adjacent_runs_when_only_rfonts_hint_differs() {
    let bytes = build_rfonts_hint_fragmented_docx();
    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");

    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].body, "2025年（第18届）");
    assert_single_editable_chunk_for_all_presets(&regions, "2025年（第18届）");
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

    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].body, "Ctrl + 0");
    assert_single_editable_chunk_for_all_presets(&regions, "Ctrl + 0");
}

#[test]
fn merges_adjacent_runs_when_hint_only_rfonts_is_mixed_with_real_style_properties() {
    let bytes = build_color_and_hint_fragmented_docx();
    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");

    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].body, "建议不超过1页");
    assert_single_editable_chunk_for_all_presets(&regions, "建议不超过1页");
}

#[test]
fn merges_writeback_regions_when_hint_only_rfonts_would_otherwise_split_text() {
    let bytes = build_color_and_hint_fragmented_docx();
    let regions = DocxAdapter::extract_writeback_regions(&bytes).expect("extract writeback");

    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].body, "建议不超过1页");
    assert_single_editable_chunk_for_all_presets(&regions, "建议不超过1页");
}

#[test]
fn report_template_keeps_shortcuts_and_page_counts_in_whole_chunks() {
    let bytes = load_repo_docx_fixture("04-3 作品报告（大数据应用赛，2025版）模板.docx");
    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");
    let chunks = rewrite::segment_regions_with_strategy(
        regions,
        ChunkPreset::Clause,
        DocumentFormat::PlainText,
        RegionSegmentationStrategy::PreserveBoundaries,
    );
    let chunk_texts = chunks
        .iter()
        .map(|chunk| format!("{}{}", chunk.text, chunk.separator_after))
        .collect::<Vec<_>>();

    assert!(chunk_texts.iter().any(|text| text.contains("Ctrl + 0")));
    assert!(chunk_texts
        .iter()
        .any(|text| text.contains("建议不超过1页")));
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
    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].body, "甲\t乙\n丙");
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
fn rejects_import_for_hyperlink_with_embedded_formula() {
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

    let error = DocxAdapter::extract_regions(&bytes, false).expect_err("expected import failure");
    assert!(error.contains("超链接内嵌公式") || error.contains("超链接"));
}

#[test]
fn rejects_import_for_hyperlink_with_page_break() {
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

    let error = DocxAdapter::extract_regions(&bytes, false).expect_err("expected import failure");
    assert!(error.contains("超链接内分页符") || error.contains("写回"));
}

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
fn rejects_embedded_office_objects_with_explicit_error() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:object/></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);

    let error = DocxAdapter::extract_regions(&bytes, false).expect_err("expected failure");
    assert!(error.contains("嵌入") || error.contains("Office"));
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
fn writes_back_repo_sample_docx_without_false_source_mismatch() {
    let bytes = include_bytes!("../../../../testdoc/chunk-test.docx");
    let source = DocxAdapter::extract_text(bytes).expect("extract text");
    let writeback_source =
        DocxAdapter::extract_writeback_source_text(bytes).expect("extract writeback source");
    let regions = DocxAdapter::extract_regions(bytes, false).expect("extract regions");

    assert_eq!(writeback_source, source);

    let rewritten = DocxAdapter::write_updated_regions(bytes, &source, &regions)
        .expect("write updated regions");
    let extracted = DocxAdapter::extract_text(&rewritten).expect("extract rewritten text");

    assert_eq!(extracted, source);
}
