use crate::{
    adapters::{tex::TexAdapter, TextRegion},
    documents::{load_document_source, writeback_slots_from_regions},
    models::{SegmentationPreset, TextPresentation},
    rewrite_unit::build_rewrite_units,
    test_support::{build_minimal_docx, build_minimal_pdf, cleanup_dir, write_temp_file},
};

fn slot_block_anchors(loaded: &crate::documents::source::LoadedDocumentSource) -> Vec<String> {
    loaded
        .writeback_slots
        .iter()
        .map(|slot| slot.anchor.as_deref().unwrap_or_default())
        .filter_map(|anchor| anchor.rsplit_once(":r").map(|(block, _)| block.to_string()))
        .collect()
}

fn unique_block_anchors(loaded: &crate::documents::source::LoadedDocumentSource) -> Vec<String> {
    let mut blocks = slot_block_anchors(loaded);
    blocks.dedup();
    blocks
}

#[test]
fn writeback_slots_split_preserved_block_separator_from_region_body() {
    let slots = writeback_slots_from_regions(&[TextRegion::editable("第一段\n\n")]);

    assert_eq!(slots.len(), 1);
    assert_eq!(slots[0].text, "第一段");
    assert_eq!(slots[0].separator_after, "\n\n");
    assert!(slots[0].editable);
}

#[test]
fn writeback_slots_lock_whitespace_only_regions_even_when_region_is_editable() {
    let underline = Some(TextPresentation {
        bold: false,
        italic: false,
        underline: true,
        href: None,
        protect_kind: None,
        writeback_key: Some("r:underline".to_string()),
    });
    let slots = writeback_slots_from_regions(&[
        TextRegion::editable("　　　\n\n").with_presentation(underline.clone())
    ]);

    assert_eq!(slots.len(), 1);
    assert_eq!(slots[0].text, "　　　");
    assert_eq!(slots[0].separator_after, "\n\n");
    assert!(!slots[0].editable);
    assert_eq!(slots[0].presentation, underline);
}

#[test]
fn writeback_slots_preserve_paragraph_boundaries_for_rewrite_units() {
    let slots = writeback_slots_from_regions(&[
        TextRegion::editable("第一段\n\n"),
        TextRegion::editable("第二段"),
    ]);

    let units = build_rewrite_units(&slots, SegmentationPreset::Paragraph);

    assert_eq!(units.len(), 2);
    assert_eq!(units[0].display_text, "第一段\n\n");
    assert_eq!(units[1].display_text, "第二段");
}

#[test]
fn tex_single_source_newline_does_not_split_paragraph_units() {
    let regions = TexAdapter::parse_regions("第一句。\n第二句。", false);
    let slots = writeback_slots_from_regions(&regions);

    let units = build_rewrite_units(&slots, SegmentationPreset::Paragraph);

    assert_eq!(units.len(), 1);
    assert_eq!(units[0].display_text, "第一句。\n第二句。");
}

#[test]
fn tex_blank_line_boundaries_split_heading_and_paragraph_units() {
    let text = "\\section{标题}\n\n第一段第一行。\n第一段第二行。\n\n第二段。";
    let regions = TexAdapter::parse_regions(text, false);
    let slots = writeback_slots_from_regions(&regions);

    let units = build_rewrite_units(&slots, SegmentationPreset::Paragraph);

    assert_eq!(units.len(), 3);
    assert_eq!(units[0].display_text, "\\section{标题}\n\n");
    assert_eq!(units[1].display_text, "第一段第一行。\n第一段第二行。\n\n");
    assert_eq!(units[2].display_text, "第二段。");
}

#[test]
fn crlf_blank_line_boundaries_split_paragraph_units() {
    let slots = writeback_slots_from_regions(&[TextRegion::editable("第一段\r\n\r\n第二段")]);

    let units = build_rewrite_units(&slots, SegmentationPreset::Paragraph);

    assert_eq!(units.len(), 2);
    assert_eq!(units[0].display_text, "第一段\r\n\r\n");
    assert_eq!(units[1].display_text, "第二段");
}

#[test]
fn load_docx_source_marks_page_break_placeholder_as_inline_object_slot() {
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
    let (root, path) = write_temp_file("docx-slot-page-break", "docx", &bytes);

    let loaded = load_document_source(&path, false).expect("load docx source");
    let slot = loaded
        .writeback_slots
        .iter()
        .find(|slot| slot.text == "[分页符]")
        .expect("page break slot");

    assert_eq!(
        slot.role,
        crate::rewrite_unit::WritebackSlotRole::InlineObject
    );
    assert!(!slot.editable);

    cleanup_dir(&root);
}

#[test]
fn load_plain_text_source_builds_stable_paragraph_anchors() {
    let (root, path) = write_temp_file("plain-template", "txt", "第一段\n\n第二段".as_bytes());

    let loaded = load_document_source(&path, false).expect("load txt");
    let anchors = loaded
        .writeback_slots
        .iter()
        .map(|slot| slot.anchor.clone().unwrap_or_default())
        .collect::<Vec<_>>();

    assert_eq!(anchors, vec!["txt:p0:r0:s0", "txt:p1:r0:s0"]);
    assert_eq!(loaded.template_kind.as_deref(), Some("plain_text"));
    assert!(loaded.template_signature.is_some());
    assert!(loaded.slot_structure_signature.is_some());

    cleanup_dir(&root);
}

#[test]
fn load_markdown_source_builds_template_metadata_and_anchors() {
    let (root, path) = write_temp_file("markdown-template", "md", "第一段\n\n第二段".as_bytes());

    let loaded = load_document_source(&path, false).expect("load markdown");
    let anchors = loaded
        .writeback_slots
        .iter()
        .map(|slot| slot.anchor.clone().unwrap_or_default())
        .collect::<Vec<_>>();

    assert_eq!(anchors, vec!["md:b0:r0:s0", "md:b1:r0:s0"]);
    assert_eq!(loaded.template_kind.as_deref(), Some("markdown"));
    assert!(loaded.template_signature.is_some());
    assert!(loaded.slot_structure_signature.is_some());

    cleanup_dir(&root);
}

#[test]
fn load_tex_source_builds_template_metadata_and_anchors() {
    let (root, path) = write_temp_file("tex-template", "tex", "第一段\n\n第二段".as_bytes());

    let loaded = load_document_source(&path, false).expect("load tex");
    let anchors = loaded
        .writeback_slots
        .iter()
        .map(|slot| slot.anchor.clone().unwrap_or_default())
        .collect::<Vec<_>>();

    assert_eq!(anchors, vec!["tex:b0:r0:s0", "tex:b1:r0:s0"]);
    assert_eq!(loaded.template_kind.as_deref(), Some("tex"));
    assert!(loaded.template_signature.is_some());
    assert!(loaded.slot_structure_signature.is_some());

    cleanup_dir(&root);
}

#[test]
fn load_pdf_source_builds_template_metadata_and_safe_capabilities() {
    let bytes = build_minimal_pdf(&["Alpha line", "Beta line"]);
    let (root, path) = write_temp_file("pdf-template", "pdf", &bytes);

    let loaded = load_document_source(&path, false).expect("load pdf");
    let anchors = loaded
        .writeback_slots
        .iter()
        .map(|slot| slot.anchor.clone().unwrap_or_default())
        .collect::<Vec<_>>();

    assert_eq!(loaded.template_kind.as_deref(), Some("pdf"));
    assert!(loaded.template_signature.is_some());
    assert!(loaded.slot_structure_signature.is_some());
    assert!(loaded.template_snapshot.is_none());
    assert_eq!(loaded.source_text, "Alpha line\nBeta line\n");
    assert_eq!(anchors, vec!["pdf:p0:b0:r0:s0", "pdf:p0:b1:r0:s0"]);
    assert!(loaded.capability_policy.source_writeback.allowed);
    assert!(loaded.capability_policy.editor_writeback.allowed);

    cleanup_dir(&root);
}

#[test]
fn load_pdf_source_blocks_rewrite_for_duplicate_chunks() {
    let bytes = build_minimal_pdf(&["Repeat", "Repeat"]);
    let (root, path) = write_temp_file("pdf-duplicate", "pdf", &bytes);

    let loaded = load_document_source(&path, false).expect("load pdf");

    assert_eq!(loaded.template_kind.as_deref(), Some("pdf"));
    assert_eq!(loaded.source_text, "Repeat\nRepeat\n");
    assert!(!loaded.capability_policy.source_writeback.allowed);
    assert!(!loaded.capability_policy.editor_writeback.allowed);
    assert!(loaded
        .capability_policy
        .source_writeback
        .block_reason
        .as_deref()
        .is_some_and(|message| message.contains("重复文本块")));

    cleanup_dir(&root);
}

#[test]
fn load_markdown_source_preserves_multiple_regions_within_one_block() {
    let (root, path) = write_temp_file("markdown-inline-block", "md", "1. 正文".as_bytes());

    let loaded = load_document_source(&path, false).expect("load markdown");
    let anchors = loaded
        .writeback_slots
        .iter()
        .map(|slot| slot.anchor.clone().unwrap_or_default())
        .collect::<Vec<_>>();

    assert_eq!(anchors, vec!["md:b0:r0:s0", "md:b0:r1:s0"]);
    assert!(!loaded.writeback_slots[0].editable);
    assert!(loaded.writeback_slots[1].editable);

    cleanup_dir(&root);
}

#[test]
fn load_markdown_source_splits_heading_and_body_into_distinct_blocks_without_blank_line() {
    let markdown = "# 标题\n正文第一行。\n";
    let (root, path) = write_temp_file("markdown-heading-body", "md", markdown.as_bytes());

    let loaded = load_document_source(&path, false).expect("load markdown");
    let block_anchors = slot_block_anchors(&loaded);

    assert_eq!(block_anchors, vec!["md:b0", "md:b1"]);
    assert!(loaded.writeback_slots[0].text.contains("标题"));
    assert!(!loaded.writeback_slots[0].editable);
    assert!(loaded
        .writeback_slots
        .iter()
        .any(|slot| slot.anchor.as_deref() == Some("md:b1:r0:s0") && slot.editable));

    cleanup_dir(&root);
}

#[test]
fn load_markdown_source_splits_adjacent_list_items_into_distinct_blocks() {
    let markdown = "1. 第一项\n2. 第二项\n";
    let (root, path) = write_temp_file("markdown-list-items", "md", markdown.as_bytes());

    let loaded = load_document_source(&path, false).expect("load markdown");
    let anchors = loaded
        .writeback_slots
        .iter()
        .map(|slot| slot.anchor.clone().unwrap_or_default())
        .collect::<Vec<_>>();

    assert_eq!(
        anchors,
        vec!["md:b0:r0:s0", "md:b0:r1:s0", "md:b1:r0:s0", "md:b1:r1:s0"]
    );
    assert!(!loaded.writeback_slots[0].editable);
    assert!(loaded.writeback_slots[1].editable);
    assert!(!loaded.writeback_slots[2].editable);
    assert!(loaded.writeback_slots[3].editable);

    cleanup_dir(&root);
}

#[test]
fn load_tex_source_preserves_command_shell_and_inner_text_in_one_block() {
    let (root, path) = write_temp_file("tex-command-block", "tex", "\\textbf{重点}".as_bytes());

    let loaded = load_document_source(&path, false).expect("load tex");
    let anchors = loaded
        .writeback_slots
        .iter()
        .map(|slot| slot.anchor.clone().unwrap_or_default())
        .collect::<Vec<_>>();

    assert_eq!(
        anchors,
        vec!["tex:b0:r0:s0", "tex:b0:r1:s0", "tex:b0:r2:s0"]
    );
    assert!(!loaded.writeback_slots[0].editable);
    assert!(loaded.writeback_slots[1].editable);
    assert!(!loaded.writeback_slots[2].editable);

    cleanup_dir(&root);
}

#[test]
fn load_tex_source_splits_section_command_and_body_into_distinct_blocks_without_blank_line() {
    let tex = "\\section{标题}\n正文开始。";
    let (root, path) = write_temp_file("tex-section-body", "tex", tex.as_bytes());

    let loaded = load_document_source(&path, false).expect("load tex");
    let block_anchors = unique_block_anchors(&loaded);

    assert_eq!(block_anchors, vec!["tex:b0", "tex:b1"]);
    assert!(loaded.writeback_slots.iter().any(|slot| slot
        .anchor
        .as_deref()
        .is_some_and(|anchor| anchor.starts_with("tex:b0:"))));
    assert!(loaded
        .writeback_slots
        .iter()
        .any(|slot| slot.anchor.as_deref() == Some("tex:b1:r0:s0") && slot.editable));

    cleanup_dir(&root);
}

#[test]
fn load_tex_source_splits_adjacent_items_into_distinct_blocks() {
    let tex = "\\begin{itemize}\n\\item 第一项\n\\item 第二项\n\\end{itemize}\n";
    let (root, path) = write_temp_file("tex-itemize-items", "tex", tex.as_bytes());

    let loaded = load_document_source(&path, false).expect("load tex");
    let block_anchors = unique_block_anchors(&loaded);

    assert_eq!(block_anchors, vec!["tex:b0", "tex:b1"]);
    assert!(loaded.writeback_slots.iter().any(|slot| slot
        .anchor
        .as_deref()
        .is_some_and(|anchor| anchor.starts_with("tex:b0:"))
        && slot.editable));
    assert!(loaded.writeback_slots.iter().any(|slot| slot
        .anchor
        .as_deref()
        .is_some_and(|anchor| anchor.starts_with("tex:b1:"))
        && slot.editable));

    cleanup_dir(&root);
}
