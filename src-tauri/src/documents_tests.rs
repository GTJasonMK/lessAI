use std::{fs, path::Path};

use super::{
    ensure_document_can_write_back, ensure_document_source_matches_session,
    execute_document_writeback, load_document_source, normalize_text_against_source_layout,
    DocumentWriteback, WritebackMode,
};
use crate::document_snapshot::{capture_document_snapshot, SNAPSHOT_MISSING_ERROR};
use crate::test_support::{build_docx_entries, build_minimal_docx, cleanup_dir, write_temp_file};

fn rebuild_source_text(loaded: &super::LoadedDocumentSource) -> String {
    loaded
        .writeback_slots
        .iter()
        .map(|slot| format!("{}{}", slot.text, slot.separator_after))
        .collect::<String>()
}

#[test]
fn decode_utf8_bom_text_file() {
    let bytes = [0xEF, 0xBB, 0xBF, b'a', b'b', b'c'];
    assert_eq!(super::source::decode_text_file(&bytes).unwrap(), "abc");
}

#[test]
fn decode_utf16_le_bom_text_file() {
    let bytes = [0xFF, 0xFE, b'A', 0x00, b'\n', 0x00];
    assert_eq!(super::source::decode_text_file(&bytes).unwrap(), "A\n");
}

#[test]
fn decode_utf16_be_bom_text_file() {
    let bytes = [0xFE, 0xFF, 0x00, b'A', 0x00, b'\n'];
    assert_eq!(super::source::decode_text_file(&bytes).unwrap(), "A\n");
}

#[test]
fn decode_invalid_text_file_returns_error() {
    let bytes = [0xFF, 0xFF, 0xFF];
    assert!(super::source::decode_text_file(&bytes).is_err());
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
    assert!(
        super::writeback::ensure_document_can_ai_rewrite(path, false, Some("pdf 不支持写回"))
            .is_ok()
    );
}

#[test]
fn docx_without_writeback_support_is_not_allowed_to_continue_ai_rewrite() {
    let path = Path::new("/tmp/demo.docx");
    let error = super::writeback::ensure_document_can_ai_rewrite(
        path,
        false,
        Some("当前 docx 暂不支持安全写回覆盖。"),
    )
    .expect_err("expected rewrite guard");

    assert!(error.contains("docx") || error.contains("写回"));
}

#[test]
fn normalize_text_against_source_layout_reuses_plain_text_layout_rules() {
    let normalized =
        normalize_text_against_source_layout("原文  \r\n下一行\r\n", "新文  \n下一行  \n");

    assert_eq!(normalized, "新文  \r\n下一行  \r\n");
}

#[test]
fn write_document_content_rejects_external_change_for_plain_text() {
    let (root, target) = write_temp_file("plain-writeback-mismatch", "txt", "原始内容".as_bytes());
    let snapshot = capture_document_snapshot(&target).expect("capture snapshot");

    fs::write(&target, "外部修改").expect("simulate external change");

    let error = execute_document_writeback(
        &target,
        "原始内容",
        Some(&snapshot),
        DocumentWriteback::Text("新的内容"),
        WritebackMode::Write,
    )
    .expect_err("expected mismatch error");
    assert!(error.contains("原文件已在外部发生变化"));

    cleanup_dir(&root);
}

#[test]
fn ensure_document_source_matches_session_rejects_external_change_for_plain_text() {
    let (root, target) =
        write_temp_file("plain-source-guard-mismatch", "txt", "原始内容".as_bytes());
    let snapshot = capture_document_snapshot(&target).expect("capture snapshot");

    fs::write(&target, "外部修改").expect("simulate external change");

    let error = ensure_document_source_matches_session(&target, Some(&snapshot))
        .expect_err("expected mismatch error");
    assert!(error.contains("原文件已在外部发生变化"));

    cleanup_dir(&root);
}

#[test]
fn write_document_content_rejects_plain_text_without_snapshot_even_when_source_matches() {
    let (root, target) = write_temp_file(
        "plain-writeback-without-snapshot",
        "txt",
        "原始内容".as_bytes(),
    );

    let error = execute_document_writeback(
        &target,
        "原始内容",
        None,
        DocumentWriteback::Text("新的内容"),
        WritebackMode::Write,
    )
    .expect_err("expected missing snapshot to be rejected");

    assert_eq!(error, SNAPSHOT_MISSING_ERROR);

    cleanup_dir(&root);
}

#[test]
fn write_document_content_rejects_docx_without_snapshot_even_when_source_matches() {
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>原文</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(document_xml);
    let (root, target) = write_temp_file("docx-writeback-without-snapshot", "docx", &bytes);

    let error = execute_document_writeback(
        &target,
        "原文",
        None,
        DocumentWriteback::Text("新正文"),
        WritebackMode::Write,
    )
    .expect_err("expected missing snapshot to be rejected");

    assert_eq!(error, SNAPSHOT_MISSING_ERROR);

    cleanup_dir(&root);
}

#[test]
fn load_markdown_source_returns_writeback_slots() {
    let markdown =
        "# 标题\n正文里的 `code` 和 [链接](https://example.com)。\n\n```ts\nconst x = 1;\n```\n";
    let (root, target) = write_temp_file("markdown-source", "md", markdown.as_bytes());

    let loaded = load_document_source(&target, false).expect("load markdown");

    assert_eq!(loaded.source_text, markdown);
    assert_eq!(rebuild_source_text(&loaded), markdown);
    assert!(loaded.writeback_slots.iter().any(|slot| !slot.editable));
    assert!(loaded
        .writeback_slots
        .iter()
        .any(|slot| slot.text.contains("`code`")));
    assert!(loaded
        .writeback_slots
        .iter()
        .any(|slot| slot.text.contains("```ts")));

    cleanup_dir(&root);
}

#[test]
fn load_tex_source_returns_writeback_slots() {
    let tex = "\\section{标题}\n正文和公式 $x+y$。\n% 注释\n";
    let (root, target) = write_temp_file("tex-source", "tex", tex.as_bytes());

    let loaded = load_document_source(&target, false).expect("load tex");

    assert_eq!(loaded.source_text, tex);
    assert_eq!(rebuild_source_text(&loaded), tex);
    assert!(loaded.writeback_slots.iter().any(|slot| !slot.editable));
    assert!(loaded
        .writeback_slots
        .iter()
        .any(|slot| slot.text.contains("\\section")));
    assert!(loaded
        .writeback_slots
        .iter()
        .any(|slot| slot.text.contains("$x+y$")));

    cleanup_dir(&root);
}

#[test]
fn load_plain_text_source_returns_single_editable_slot() {
    let text = "第一句。\n第二句。";
    let (root, target) = write_temp_file("plain-source", "txt", text.as_bytes());

    let loaded = load_document_source(&target, false).expect("load text");

    assert_eq!(loaded.source_text, text);
    assert_eq!(loaded.writeback_slots.len(), 1);
    assert_eq!(loaded.writeback_slots[0].text, text);
    assert!(loaded.writeback_slots[0].editable);

    cleanup_dir(&root);
}

#[test]
fn load_docx_source_preserves_writeback_slot_boundaries() {
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
    let (root, target) = write_temp_file("docx-source", "docx", &bytes);

    let loaded = load_document_source(&target, false).expect("load docx");

    assert_eq!(rebuild_source_text(&loaded), loaded.source_text);
    assert!(loaded.writeback_slots.iter().any(|slot| !slot.editable));
    assert!(loaded
        .writeback_slots
        .iter()
        .any(|slot| slot.text.contains("标题")));
    assert!(loaded
        .writeback_slots
        .iter()
        .any(|slot| slot.text.contains("正文")));

    cleanup_dir(&root);
}
