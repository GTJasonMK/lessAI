use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
};

use uuid::Uuid;
use zip::{write::FileOptions, ZipWriter};

use super::{
    decode_text_file, ensure_document_can_ai_rewrite, ensure_document_can_write_back,
    ensure_document_source_matches_session, load_document_source, write_document_content,
    RegionSegmentationStrategy,
};
use crate::document_snapshot::capture_document_snapshot;

fn unique_test_dir(name: &str) -> PathBuf {
    env::temp_dir().join(format!("lessai-{name}-{}", Uuid::new_v4()))
}

fn cleanup_dir(path: &Path) {
    let _ = fs::remove_dir_all(path);
}

fn write_temp_file(name: &str, ext: &str, contents: &[u8]) -> (PathBuf, PathBuf) {
    let root = unique_test_dir(name);
    fs::create_dir_all(&root).expect("create root");
    let target = root.join(format!("sample.{ext}"));
    fs::write(&target, contents).expect("write temp file");
    (root, target)
}

fn build_minimal_docx(document_xml: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let cursor = std::io::Cursor::new(&mut out);
    let mut zip = ZipWriter::new(cursor);
    let options = FileOptions::<()>::default();
    zip.start_file("word/document.xml", options)
        .expect("start document.xml");
    zip.write_all(document_xml.as_bytes())
        .expect("write document.xml");
    zip.finish().expect("finish docx");
    out
}

fn rebuild_regions_text(loaded: &super::LoadedDocumentSource) -> String {
    loaded
        .regions
        .iter()
        .map(|region| region.body.as_str())
        .collect::<String>()
}

#[test]
fn decode_utf8_bom_text_file() {
    let bytes = [0xEF, 0xBB, 0xBF, b'a', b'b', b'c'];
    assert_eq!(decode_text_file(&bytes).unwrap(), "abc");
}

#[test]
fn decode_utf16_le_bom_text_file() {
    let bytes = [0xFF, 0xFE, b'A', 0x00, b'\n', 0x00];
    assert_eq!(decode_text_file(&bytes).unwrap(), "A\n");
}

#[test]
fn decode_utf16_be_bom_text_file() {
    let bytes = [0xFE, 0xFF, 0x00, b'A', 0x00, b'\n'];
    assert_eq!(decode_text_file(&bytes).unwrap(), "A\n");
}

#[test]
fn decode_invalid_text_file_returns_error() {
    let bytes = [0xFF, 0xFF, 0xFF];
    assert!(decode_text_file(&bytes).is_err());
}

#[test]
fn docx_is_allowed_to_write_back() {
    assert!(ensure_document_can_write_back("/tmp/demo.docx").is_ok());
}

#[test]
fn pdf_is_not_allowed_to_write_back() {
    assert!(ensure_document_can_write_back("/tmp/demo.pdf").is_err());
}

#[test]
fn pdf_is_allowed_to_continue_ai_rewrite_without_writeback() {
    let path = Path::new("/tmp/demo.pdf");
    assert!(ensure_document_can_ai_rewrite(path, false, Some("pdf 不支持写回")).is_ok());
}

#[test]
fn docx_without_writeback_support_is_not_allowed_to_continue_ai_rewrite() {
    let path = Path::new("/tmp/demo.docx");
    let error =
        ensure_document_can_ai_rewrite(path, false, Some("当前 docx 暂不支持安全写回覆盖。"))
            .expect_err("expected rewrite guard");

    assert!(error.contains("docx") || error.contains("写回"));
}

#[test]
fn write_document_content_rejects_external_change_for_plain_text() {
    let root = unique_test_dir("plain-writeback-mismatch");
    fs::create_dir_all(&root).expect("create root");
    let target = root.join("draft.txt");
    fs::write(&target, "原始内容").expect("seed text file");
    let snapshot = capture_document_snapshot(&target).expect("capture snapshot");

    fs::write(&target, "外部修改").expect("simulate external change");

    let error = write_document_content(&target, "原始内容", Some(&snapshot), "新的内容")
        .expect_err("expected mismatch error");
    assert!(error.contains("原文件已在外部发生变化"));

    cleanup_dir(&root);
}

#[test]
fn ensure_document_source_matches_session_rejects_external_change_for_plain_text() {
    let root = unique_test_dir("plain-source-guard-mismatch");
    fs::create_dir_all(&root).expect("create root");
    let target = root.join("draft.txt");
    fs::write(&target, "原始内容").expect("seed text file");
    let snapshot = capture_document_snapshot(&target).expect("capture snapshot");

    fs::write(&target, "外部修改").expect("simulate external change");

    let error = ensure_document_source_matches_session(&target, "原始内容", Some(&snapshot))
        .expect_err("expected mismatch error");
    assert!(error.contains("原文件已在外部发生变化"));

    cleanup_dir(&root);
}

#[test]
fn write_document_content_allows_plain_text_without_snapshot_when_source_matches() {
    let root = unique_test_dir("plain-writeback-without-snapshot");
    fs::create_dir_all(&root).expect("create root");
    let target = root.join("draft.txt");
    fs::write(&target, "原始内容").expect("seed text file");

    write_document_content(&target, "原始内容", None, "新的内容")
        .expect("write without snapshot when source matches");

    let written = fs::read_to_string(&target).expect("read written file");
    assert_eq!(written, "新的内容");

    cleanup_dir(&root);
}

#[test]
fn write_document_content_allows_docx_without_snapshot_when_source_matches() {
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>原文</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(document_xml);
    let (root, target) = write_temp_file("docx-writeback-without-snapshot", "docx", &bytes);

    write_document_content(&target, "原文", None, "新正文")
        .expect("docx write without snapshot when source matches");

    let loaded = load_document_source(&target, false).expect("reload docx");
    assert_eq!(loaded.source_text, "新正文");

    cleanup_dir(&root);
}

#[test]
fn load_markdown_source_returns_regions_with_format_aware_strategy() {
    let markdown =
        "# 标题\n正文里的 `code` 和 [链接](https://example.com)。\n\n```ts\nconst x = 1;\n```\n";
    let (root, target) = write_temp_file("markdown-source", "md", markdown.as_bytes());

    let loaded = load_document_source(&target, false).expect("load markdown");

    assert_eq!(loaded.source_text, markdown);
    assert_eq!(
        loaded.region_segmentation_strategy,
        RegionSegmentationStrategy::FormatAware
    );
    assert_eq!(rebuild_regions_text(&loaded), markdown);
    assert!(loaded.regions.iter().any(|region| region.skip_rewrite));
    assert!(loaded
        .regions
        .iter()
        .any(|region| region.body.contains("`code`")));
    assert!(loaded
        .regions
        .iter()
        .any(|region| region.body.contains("```ts")));

    cleanup_dir(&root);
}

#[test]
fn load_tex_source_returns_regions_with_format_aware_strategy() {
    let tex = "\\section{标题}\n正文和公式 $x+y$。\n% 注释\n";
    let (root, target) = write_temp_file("tex-source", "tex", tex.as_bytes());

    let loaded = load_document_source(&target, false).expect("load tex");

    assert_eq!(loaded.source_text, tex);
    assert_eq!(
        loaded.region_segmentation_strategy,
        RegionSegmentationStrategy::FormatAware
    );
    assert_eq!(rebuild_regions_text(&loaded), tex);
    assert!(loaded.regions.iter().any(|region| region.skip_rewrite));
    assert!(loaded
        .regions
        .iter()
        .any(|region| region.body.contains("\\section")));
    assert!(loaded
        .regions
        .iter()
        .any(|region| region.body.contains("$x+y$")));

    cleanup_dir(&root);
}

#[test]
fn load_plain_text_source_returns_single_editable_region() {
    let text = "第一句。\n第二句。";
    let (root, target) = write_temp_file("plain-source", "txt", text.as_bytes());

    let loaded = load_document_source(&target, false).expect("load text");

    assert_eq!(loaded.source_text, text);
    assert_eq!(
        loaded.region_segmentation_strategy,
        RegionSegmentationStrategy::FormatAware
    );
    assert_eq!(loaded.regions.len(), 1);
    assert_eq!(loaded.regions[0].body, text);
    assert!(!loaded.regions[0].skip_rewrite);

    cleanup_dir(&root);
}

#[test]
fn load_docx_source_preserves_region_boundaries() {
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:pPr><w:pStyle w:val="Heading1"/></w:pPr>
      <w:r><w:t>标题</w:t></w:r>
    </w:p>
    <w:p><w:r><w:t>正文</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(document_xml);
    let (root, target) = write_temp_file("docx-source", "docx", &bytes);

    let loaded = load_document_source(&target, false).expect("load docx");

    assert_eq!(
        loaded.region_segmentation_strategy,
        RegionSegmentationStrategy::PreserveBoundaries
    );
    assert_eq!(rebuild_regions_text(&loaded), loaded.source_text);
    assert!(loaded.regions.iter().any(|region| region.skip_rewrite));
    assert!(loaded
        .regions
        .iter()
        .any(|region| region.body.contains("标题")));
    assert!(loaded
        .regions
        .iter()
        .any(|region| region.body.contains("正文")));

    cleanup_dir(&root);
}
