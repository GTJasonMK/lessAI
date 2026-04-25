use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
};

use chrono::Utc;
use uuid::Uuid;
use zip::{write::FileOptions, ZipWriter};

use crate::{
    models::{
        DiffResult, DocumentSession, RewriteUnitStatus, RunningState, SegmentationPreset,
        SuggestionDecision,
    },
    rewrite_unit::{RewriteSuggestion, RewriteUnit, SlotUpdate, WritebackSlot},
    session_capability_models::{CapabilityGate, DocumentSessionCapabilities},
};

pub(crate) fn unique_test_dir(name: &str) -> PathBuf {
    env::temp_dir().join(format!("lessai-{name}-{}", Uuid::new_v4()))
}

pub(crate) fn cleanup_dir(path: &Path) {
    let _ = fs::remove_dir_all(path);
}

pub(crate) fn write_temp_file(name: &str, ext: &str, contents: &[u8]) -> (PathBuf, PathBuf) {
    let root = unique_test_dir(name);
    fs::create_dir_all(&root).expect("create root");
    let target = root.join(format!("sample.{ext}"));
    fs::write(&target, contents).expect("write temp file");
    (root, target)
}

pub(crate) fn build_docx_entries(entries: &[(&str, &str)]) -> Vec<u8> {
    let mut out = Vec::new();
    let cursor = std::io::Cursor::new(&mut out);
    let mut zip = ZipWriter::new(cursor);
    let options = FileOptions::<()>::default();

    for (name, contents) in entries {
        zip.start_file(*name, options).expect("start zip entry");
        zip.write_all(contents.as_bytes()).expect("write zip entry");
    }

    zip.finish().expect("finish docx");
    out
}

pub(crate) fn build_minimal_docx(document_xml: &str) -> Vec<u8> {
    build_docx_entries(&[("word/document.xml", document_xml)])
}

pub(crate) fn build_minimal_pdf(lines: &[&str]) -> Vec<u8> {
    build_minimal_pdf_with_features(lines, false, false)
}

pub(crate) fn build_minimal_pdf_with_features(
    lines: &[&str],
    include_graphics_path: bool,
    include_link_annotation: bool,
) -> Vec<u8> {
    use lopdf::{
        content::{Content, Operation},
        dictionary, Document, Object, Stream,
    };

    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();

    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let resources_id = doc.add_object(dictionary! {
        "Font" => dictionary! {
            "F1" => font_id,
        },
    });

    let mut operations = Vec::new();
    if include_graphics_path {
        operations.push(Operation::new(
            "re",
            vec![72.into(), 680.into(), 120.into(), 40.into()],
        ));
        operations.push(Operation::new("S", vec![]));
    }
    for (index, line) in lines.iter().enumerate() {
        operations.push(Operation::new("BT", vec![]));
        operations.push(Operation::new("Tf", vec!["F1".into(), 14.into()]));
        operations.push(Operation::new(
            "Td",
            vec![72.into(), (760_i64 - (index as i64 * 24)).into()],
        ));
        operations.push(Operation::new("Tj", vec![Object::string_literal(*line)]));
        operations.push(Operation::new("ET", vec![]));
    }
    let content = Content { operations };
    let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));

    let mut page = dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
    };
    if include_link_annotation {
        let action_id = doc.add_object(dictionary! {
            "S" => "URI",
            "URI" => Object::string_literal("https://example.com"),
        });
        let annotation_id = doc.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Link",
            "Rect" => vec![72.into(), 680.into(), 192.into(), 720.into()],
            "Border" => vec![0.into(), 0.into(), 0.into()],
            "A" => action_id,
        });
        page.set("Annots", vec![annotation_id.into()]);
    }
    let page_id = doc.add_object(page);

    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        }),
    );

    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);

    let mut output = Vec::new();
    doc.save_to(&mut output).expect("save pdf");
    output
}

pub(crate) fn load_repo_docx_fixture_or<F>(file_name: &str, fallback: F) -> Vec<u8>
where
    F: FnOnce() -> Vec<u8>,
{
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("testdoc")
        .join(file_name);
    fs::read(path).unwrap_or_else(|_| fallback())
}

pub(crate) fn build_chunk_test_fixture_docx() -> Vec<u8> {
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>段落一：用于 writeback 来源一致性校验。</w:t></w:r></w:p>
    <w:p><w:r><w:t>段落二：保持稳定文本，不触发误判。</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
    build_minimal_docx(document_xml)
}

pub(crate) fn build_report_template_fixture_docx() -> Vec<u8> {
    let document_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"
            xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"
            xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
            xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"
            xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"
            xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">
  <w:body>
    <w:p>
      <w:pPr><w:pStyle w:val="Heading1"/></w:pPr>
      <w:r><w:t>作品概述</w:t></w:r>
    </w:p>
    <w:p>
      <w:r>
        <w:drawing>
          <wp:inline>
            <a:graphic>
              <a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture">
                <pic:pic/>
              </a:graphicData>
            </a:graphic>
          </wp:inline>
        </w:drawing>
      </w:r>
    </w:p>
    <w:p>
      <w:r>
        <w:drawing>
          <wp:anchor>
            <wp:positionV relativeFrom="page"><wp:posOffset>2400</wp:posOffset></wp:positionV>
            <a:graphic>
              <a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">
                <wps:wsp>
                  <wps:txbx>
                    <w:txbxContent>
                      <w:p><w:r><w:t>填写说明</w:t></w:r></w:p>
                    </w:txbxContent>
                  </wps:txbx>
                </wps:wsp>
              </a:graphicData>
            </a:graphic>
          </wp:anchor>
        </w:drawing>
      </w:r>
      <w:r><w:t>填写日期：</w:t></w:r>
    </w:p>
    <w:tbl>
      <w:tr>
        <w:tc><w:p><w:r><w:t>表格占位</w:t></w:r></w:p></w:tc>
      </w:tr>
    </w:tbl>
    <w:sdt>
      <w:sdtPr><w:alias w:val="普通内容控件"/></w:sdtPr>
      <w:sdtContent>
        <w:p><w:r><w:t>控件内容</w:t></w:r></w:p>
      </w:sdtContent>
    </w:sdt>
    <w:p><w:r><w:t>快捷键示例：Ctrl + 0，建议控制在1页内。</w:t></w:r></w:p>
    <w:p><w:r><w:t>作品功能需求主要包括用户登录、数据处理、报表导出。系统性能需求包括响应时间、吞吐量、可用性。</w:t></w:r></w:p>
    <w:p><w:r><w:t>本作品所使用的数据集主要由公开数据和自采数据两部分构成。数据类型涵盖结构化表格数据、文本数据。</w:t></w:r></w:p>
    <w:p>
      <w:r><w:t>以下为数据样例：</w:t></w:r>
      <w:r><w:br/></w:r>
      <w:r><w:t>样例1（表格数据）：</w:t></w:r>
      <w:r><w:br/></w:r>
      <w:r><w:t>001, 2024-01-15, 类型1, 23.5, 87.2, 正常</w:t></w:r>
    </w:p>
    <w:p>
      <w:r><w:t>样例2（JSON数据）：</w:t></w:r>
      <w:r><w:br/></w:r>
      <w:r><w:t>time: 2024-01-17 10:23:15</w:t></w:r>
      <w:r><w:br/></w:r>
      <w:r><w:t>label: 正常</w:t></w:r>
    </w:p>
    <w:p><w:r><w:t>正文段落一用于保证模板具备足够编辑区域。</w:t></w:r></w:p>
    <w:p><w:r><w:t>正文段落二用于覆盖多段落连续写回行为。</w:t></w:r></w:p>
    <w:p><w:r><w:t>正文段落三用于覆盖段内标点切块行为。</w:t></w:r></w:p>
    <w:p><w:r><w:t>正文段落四用于覆盖可编辑区域数量阈值。</w:t></w:r></w:p>
    <w:p><w:r><w:t>正文段落五用于覆盖审阅区定位稳定性。</w:t></w:r></w:p>
    <w:p><w:r><w:t>正文段落六用于覆盖导入后回写一致性。</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
    let styles_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:style w:type="paragraph" w:styleId="Heading1">
    <w:name w:val="heading 1"/>
    <w:pPr><w:numPr><w:numId w:val="1"/></w:numPr></w:pPr>
  </w:style>
</w:styles>"#;
    let numbering_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:abstractNum w:abstractNumId="0">
    <w:lvl w:ilvl="0">
      <w:start w:val="1"/>
      <w:numFmt w:val="decimal"/>
      <w:lvlText w:val="第%1章"/>
      <w:suff w:val="space"/>
    </w:lvl>
  </w:abstractNum>
  <w:num w:numId="1">
    <w:abstractNumId w:val="0"/>
  </w:num>
</w:numbering>"#;
    build_docx_entries(&[
        ("word/document.xml", document_xml),
        ("word/styles.xml", styles_xml),
        ("word/numbering.xml", numbering_xml),
    ])
}

pub(crate) fn sample_clean_session(
    id: &str,
    document_path: &str,
    source_text: &str,
) -> DocumentSession {
    let now = Utc::now();
    let mut session = DocumentSession {
        id: id.to_string(),
        title: "示例".to_string(),
        document_path: document_path.to_string(),
        source_text: source_text.to_string(),
        source_snapshot: None,
        template_kind: None,
        template_signature: None,
        slot_structure_signature: None,
        template_snapshot: None,
        normalized_text: source_text.to_string(),
        capabilities: DocumentSessionCapabilities {
            source_writeback: CapabilityGate::allowed(),
            editor_writeback: CapabilityGate::allowed(),
            ..Default::default()
        },
        segmentation_preset: Some(SegmentationPreset::Paragraph),
        rewrite_headings: Some(false),
        writeback_slots: Vec::new(),
        rewrite_units: Vec::new(),
        suggestions: Vec::new(),
        next_suggestion_sequence: 1,
        status: RunningState::Idle,
        created_at: now,
        updated_at: now,
    };
    crate::documents::hydrate_session_capabilities(&mut session);
    session
}

pub(crate) fn editable_slot(id: &str, order: usize, text: &str) -> WritebackSlot {
    WritebackSlot {
        id: id.to_string(),
        order,
        text: text.to_string(),
        editable: true,
        role: crate::rewrite_unit::WritebackSlotRole::EditableText,
        presentation: None,
        anchor: None,
        separator_after: String::new(),
    }
}

pub(crate) fn locked_slot(id: &str, order: usize, text: &str) -> WritebackSlot {
    WritebackSlot {
        id: id.to_string(),
        order,
        text: text.to_string(),
        editable: false,
        role: crate::rewrite_unit::WritebackSlotRole::LockedText,
        presentation: None,
        anchor: None,
        separator_after: String::new(),
    }
}

pub(crate) fn rewrite_unit(
    id: &str,
    order: usize,
    slot_ids: &[&str],
    display_text: &str,
    status: RewriteUnitStatus,
) -> RewriteUnit {
    RewriteUnit {
        id: id.to_string(),
        order,
        slot_ids: slot_ids.iter().map(|slot_id| slot_id.to_string()).collect(),
        display_text: display_text.to_string(),
        segmentation_preset: SegmentationPreset::Paragraph,
        status,
        error_message: None,
    }
}

pub(crate) fn rewrite_suggestion(
    id: &str,
    sequence: u64,
    rewrite_unit_id: &str,
    before_text: &str,
    after_text: &str,
    decision: SuggestionDecision,
    slot_updates: Vec<SlotUpdate>,
) -> RewriteSuggestion {
    let now = Utc::now();
    RewriteSuggestion {
        id: id.to_string(),
        sequence,
        rewrite_unit_id: rewrite_unit_id.to_string(),
        before_text: before_text.to_string(),
        after_text: after_text.to_string(),
        diff: DiffResult::default(),
        decision,
        slot_updates,
        created_at: now,
        updated_at: now,
    }
}
