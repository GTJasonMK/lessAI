use std::path::Path;

use crate::{
    adapters,
    documents::{is_docx_path, load_verified_writeback_bytes},
    models,
};

pub(crate) fn validate_document_content_writeback(
    path: &Path,
    expected_source_text: &str,
    expected_source_snapshot: Option<&models::DocumentSnapshot>,
    updated_text: &str,
) -> Result<(), String> {
    let current_bytes =
        load_verified_writeback_bytes(path, expected_source_text, expected_source_snapshot)?;
    if !is_docx_path(path) {
        return Ok(());
    }
    adapters::docx::DocxAdapter::write_updated_text(
        &current_bytes,
        expected_source_text,
        updated_text,
    )
    .map(|_| ())
}

pub(crate) fn validate_document_region_writeback(
    path: &Path,
    expected_source_text: &str,
    expected_source_snapshot: Option<&models::DocumentSnapshot>,
    updated_regions: &[adapters::TextRegion],
) -> Result<(), String> {
    let current_bytes =
        load_verified_writeback_bytes(path, expected_source_text, expected_source_snapshot)?;
    if !is_docx_path(path) {
        return Err("当前仅 docx 支持按片段校验写回。".to_string());
    }
    adapters::docx::DocxAdapter::write_updated_regions(
        &current_bytes,
        expected_source_text,
        updated_regions,
    )
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use std::{
        env, fs,
        io::Write,
        path::{Path, PathBuf},
    };

    use uuid::Uuid;
    use zip::{write::FileOptions, ZipWriter};

    use super::validate_document_content_writeback;

    fn unique_test_dir(name: &str) -> PathBuf {
        env::temp_dir().join(format!("lessai-edit-validate-{name}-{}", Uuid::new_v4()))
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
    fn rejects_docx_edit_validation_when_paragraph_count_changes() {
        let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>第一段</w:t></w:r></w:p>
    <w:p><w:r><w:t>第二段</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
        let bytes = build_minimal_docx(document_xml);
        let (root, target) = write_temp_file("paragraph-count-fail", "docx", &bytes);

        let error = validate_document_content_writeback(
            &target,
            "第一段\n\n第二段",
            None,
            "第一段\n\n新增段\n\n第二段",
        )
        .expect_err("expected paragraph count validation failure");

        assert!(error.contains("段落数量保持不变") || error.contains("简单 docx"));

        cleanup_dir(&root);
    }

    #[test]
    fn allows_docx_edit_validation_when_structure_stays_compatible() {
        let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>第一段</w:t></w:r></w:p>
    <w:p><w:r><w:t>第二段</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
        let bytes = build_minimal_docx(document_xml);
        let (root, target) = write_temp_file("paragraph-count-pass", "docx", &bytes);

        validate_document_content_writeback(
            &target,
            "第一段\n\n第二段",
            None,
            "改写第一段\n\n改写第二段",
        )
        .expect("expected structure-compatible edit to validate");

        cleanup_dir(&root);
    }
}
