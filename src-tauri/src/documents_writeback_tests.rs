use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
};

use uuid::Uuid;
use zip::{write::FileOptions, ZipWriter};

use super::{load_document_source, write_document_content};

fn unique_test_dir(name: &str) -> PathBuf {
    env::temp_dir().join(format!("lessai-doc-writeback-{name}-{}", Uuid::new_v4()))
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

#[test]
fn write_document_content_allows_docx_when_styled_prefix_becomes_empty() {
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:r><w:rPr><w:b/></w:rPr><w:t>标题</w:t></w:r>
      <w:r><w:t>正文</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(document_xml);
    let (root, target) = write_temp_file("docx-empty-styled-prefix", "docx", &bytes);

    write_document_content(&target, "标题正文", None, "正文")
        .expect("docx write should preserve empty styled boundary safely");

    let loaded = load_document_source(&target, false).expect("reload docx");
    assert_eq!(loaded.source_text, "正文");

    cleanup_dir(&root);
}

#[test]
fn write_document_content_allows_docx_with_paragraph_level_drawing_placeholder() {
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
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
    let bytes = build_minimal_docx(document_xml);
    let (root, target) = write_temp_file("docx-paragraph-level-drawing", "docx", &bytes);

    write_document_content(&target, "前文[图表]后文", None, "新前文[图表]新后文")
        .expect("docx write should preserve paragraph-level drawing placeholder safely");

    let loaded = load_document_source(&target, false).expect("reload docx");
    assert_eq!(loaded.source_text, "新前文[图表]新后文");

    cleanup_dir(&root);
}
