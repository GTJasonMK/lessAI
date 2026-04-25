use super::{
    execute_document_writeback, load_document_source, DocumentWriteback, DocumentWritebackContext,
    WritebackMode,
};
use crate::{
    document_snapshot::capture_document_snapshot,
    rewrite_unit::WritebackSlot,
    test_support::{build_minimal_pdf, cleanup_dir, write_temp_file},
};

struct TextFixture {
    name: &'static str,
    extension: &'static str,
    content: &'static str,
    rewrite_headings: bool,
}

const TEXT_FIXTURES: &[TextFixture] = &[
    TextFixture {
        name: "plain-multiline",
        extension: "txt",
        content: include_str!("../../test-fixtures/roundtrip/plain/multiline.txt"),
        rewrite_headings: false,
    },
    TextFixture {
        name: "markdown-nested-quote",
        extension: "md",
        content: include_str!("../../test-fixtures/roundtrip/markdown/nested-quote.md"),
        rewrite_headings: false,
    },
    TextFixture {
        name: "markdown-mixed-inline",
        extension: "md",
        content: include_str!("../../test-fixtures/roundtrip/markdown/mixed-inline.md"),
        rewrite_headings: false,
    },
    TextFixture {
        name: "tex-multiline-command",
        extension: "tex",
        content: include_str!("../../test-fixtures/roundtrip/tex/multiline-command.tex"),
        rewrite_headings: true,
    },
    TextFixture {
        name: "tex-nested-command",
        extension: "tex",
        content: include_str!("../../test-fixtures/roundtrip/tex/nested-command.tex"),
        rewrite_headings: false,
    },
];

#[test]
fn textual_roundtrip_fixtures_preserve_slot_structure() {
    for fixture in TEXT_FIXTURES {
        assert_text_roundtrip_fixture(fixture);
    }
}

#[test]
fn docx_roundtrip_preserves_slot_structure_signature() {
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
            xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
            xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math">
  <w:body>
    <w:p>
      <w:r><w:t>导语</w:t></w:r>
      <w:hyperlink r:id="rId1"><w:r><w:t>链接标题</w:t></w:r></w:hyperlink>
      <m:oMath><m:r><m:t>x+y</m:t></m:r></m:oMath>
      <w:r><w:t>结尾。</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com" TargetMode="External"/>
</Relationships>"#;
    let bytes = build_minimal_docx_with_rels(document_xml, rels);
    let (root, path) = write_temp_file("docx-roundtrip", "docx", &bytes);

    let loaded = load_document_source(&path, false).expect("load docx");
    let snapshot = capture_document_snapshot(&path).expect("capture snapshot");
    let updated_slots = mutate_editable_slots(&loaded.writeback_slots);

    execute_document_writeback(
        &path,
        DocumentWritebackContext::new(&loaded.source_text, Some(&snapshot))
            .with_structure_signatures(
                loaded.template_signature.as_deref(),
                loaded.slot_structure_signature.as_deref(),
                false,
            ),
        DocumentWriteback::Slots(&updated_slots),
        WritebackMode::Write,
    )
    .expect("docx roundtrip should succeed");

    let reloaded = load_document_source(&path, false).expect("reload docx");
    assert_eq!(
        reloaded.slot_structure_signature,
        loaded.slot_structure_signature
    );
    assert_eq!(
        editable_slot_count(&reloaded.writeback_slots),
        editable_slot_count(&loaded.writeback_slots)
    );
    assert!(reloaded.source_text.contains("〔改〕"));

    cleanup_dir(&root);
}

#[test]
fn safe_pdf_roundtrip_preserves_source_projection() {
    let bytes = build_minimal_pdf(&["Alpha line", "Beta line"]);
    let (root, path) = write_temp_file("pdf-roundtrip", "pdf", &bytes);

    let loaded = load_document_source(&path, false).expect("load pdf");
    let snapshot = capture_document_snapshot(&path).expect("capture snapshot");
    let mut updated_slots = loaded.writeback_slots.clone();
    let first_editable = updated_slots
        .iter_mut()
        .find(|slot| slot.editable)
        .expect("editable pdf slot");
    first_editable.text = format!("{} [rev]", first_editable.text);

    execute_document_writeback(
        &path,
        DocumentWritebackContext::new(&loaded.source_text, Some(&snapshot))
            .with_structure_signatures(
                loaded.template_signature.as_deref(),
                loaded.slot_structure_signature.as_deref(),
                false,
            ),
        DocumentWriteback::Slots(&updated_slots),
        WritebackMode::Write,
    )
    .expect("pdf roundtrip should succeed");

    let reloaded = load_document_source(&path, false).expect("reload pdf");
    assert!(reloaded.source_text.contains("[rev]"));

    cleanup_dir(&root);
}

fn assert_text_roundtrip_fixture(fixture: &TextFixture) {
    let (root, path) = write_temp_file(fixture.name, fixture.extension, fixture.content.as_bytes());
    let loaded = load_document_source(&path, fixture.rewrite_headings).expect("load fixture");
    let snapshot = capture_document_snapshot(&path).expect("capture snapshot");
    let updated_slots = mutate_editable_slots(&loaded.writeback_slots);

    execute_document_writeback(
        &path,
        DocumentWritebackContext::new(&loaded.source_text, Some(&snapshot))
            .with_structure_signatures(
                loaded.template_signature.as_deref(),
                loaded.slot_structure_signature.as_deref(),
                fixture.rewrite_headings,
            ),
        DocumentWriteback::Slots(&updated_slots),
        WritebackMode::Write,
    )
    .expect("textual roundtrip should succeed");

    let reloaded = load_document_source(&path, fixture.rewrite_headings).expect("reload fixture");
    assert_eq!(
        reloaded.slot_structure_signature,
        loaded.slot_structure_signature
    );
    assert_eq!(reloaded.writeback_slots.len(), loaded.writeback_slots.len());
    assert_eq!(
        editable_slot_count(&reloaded.writeback_slots),
        editable_slot_count(&loaded.writeback_slots)
    );
    assert!(reloaded.source_text.contains("〔改〕"));

    cleanup_dir(&root);
}

fn mutate_editable_slots(slots: &[WritebackSlot]) -> Vec<WritebackSlot> {
    let mut updated = slots.to_vec();
    let mut changed = 0usize;

    for slot in &mut updated {
        if !slot.editable || slot.text.trim().is_empty() {
            continue;
        }
        slot.text = format!("{}〔改〕", slot.text);
        changed += 1;
        if changed == 2 {
            break;
        }
    }

    assert!(changed > 0, "fixture should expose editable slots");
    updated
}

fn editable_slot_count(slots: &[WritebackSlot]) -> usize {
    slots.iter().filter(|slot| slot.editable).count()
}

fn build_minimal_docx_with_rels(document_xml: &str, rels_xml: &str) -> Vec<u8> {
    crate::test_support::build_docx_entries(&[
        ("word/document.xml", document_xml),
        ("word/_rels/document.xml.rels", rels_xml),
    ])
}
