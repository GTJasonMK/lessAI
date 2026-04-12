# Docx Template Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Support importing `testdoc/04-3 作品报告（大数据应用赛，2025版）模板.docx`, expose only article text as editable, render textbox/TOC/image/table objects as locked placeholders, and keep safe `.docx` writeback.

**Architecture:** Keep `DocxAdapter` as the public entry point, but stop treating all non-simple Word objects as fatal. Classify article text into editable regions, classify layout-driven objects into locked raw-event placeholders, and ignore safe metadata markers such as bookmarks and proofing tags. Reuse the existing region-based writeback path so only editable regions can change, while locked placeholders round-trip unchanged.

**Tech Stack:** Rust, `quick-xml`, `zip`, existing `DocxAdapter` region/writeback model, Rust unit tests under `src-tauri/src/adapters/docx/tests.rs`

---

## File Map

- Modify: `src-tauri/src/adapters/docx.rs`
  Purpose: wire new helper modules into the existing docx adapter entry point.

- Modify: `src-tauri/src/adapters/docx/model.rs`
  Purpose: keep placeholder/writeback model definitions aligned with new locked block kinds if helper constructors need shared types.

- Create: `src-tauri/src/adapters/docx/placeholders.rs`
  Purpose: centralize locked placeholder text, `protect_kind`, and raw-event wrapper builders for image/textbox/toc/table/section/formula blocks.

- Create: `src-tauri/src/adapters/docx/subtree.rs`
  Purpose: shared XML subtree capture, local-tag scanning, and helper predicates used by both import parsing and writeback template parsing.

- Modify: `src-tauri/src/adapters/docx/simple.rs`
  Purpose: keep `DocxAdapter` public methods, but delegate complex block/paragraph classification to helper functions; import and writeback should accept article-semantic structures plus locked objects instead of only “simple docx”.

- Modify: `src-tauri/src/adapters/docx/tests.rs`
  Purpose: add fixture-based regressions for the report template, metadata-tolerant parsing, locked placeholder import, and safe writeback round-trip.

- Modify: `docs/testing-guide.md`
  Purpose: add manual verification for the report-template `.docx` scenario.

### Task 1: Fixture Import Regression

**Files:**
- Modify: `src-tauri/src/adapters/docx.rs`
- Create: `src-tauri/src/adapters/docx/placeholders.rs`
- Modify: `src-tauri/src/adapters/docx/simple.rs`
- Modify: `src-tauri/src/adapters/docx/tests.rs`
- Test: `src-tauri/src/adapters/docx/tests.rs`

- [ ] **Step 1: Write the failing fixture import test**

Add a reusable fixture loader and an import regression near the existing docx tests:

```rust
use std::{fs, path::PathBuf};

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

#[test]
fn imports_report_template_with_locked_non_article_objects() {
    let bytes = load_repo_docx_fixture("04-3 作品报告（大数据应用赛，2025版）模板.docx");

    let regions = DocxAdapter::extract_regions(&bytes, false).expect("import template");

    assert!(regions.iter().any(|r| !r.skip_rewrite && r.body.contains("作品名称")));
    assert!(regions.iter().any(|r| protect_kind_of(r) == Some("image")));
    assert!(regions.iter().any(|r| protect_kind_of(r) == Some("textbox")));
    assert!(regions.iter().any(|r| protect_kind_of(r) == Some("toc")));
    assert!(regions.iter().any(|r| protect_kind_of(r) == Some("table")));
}
```

- [ ] **Step 2: Run the targeted test to verify it fails**

Run:

```bash
cd src-tauri && cargo test imports_report_template_with_locked_non_article_objects -- --nocapture
```

Expected: FAIL with an unsupported-structure error mentioning `drawing`, `sdt`, or another currently rejected template tag.

- [ ] **Step 3: Add locked placeholder helper module**

Create `src-tauri/src/adapters/docx/placeholders.rs`:

```rust
use quick_xml::events::Event;

use super::model::{LockedRegionRender, LockedRegionTemplate, WritebackBlockTemplate};
use crate::models::ChunkPresentation;

pub(super) const DOCX_IMAGE_PLACEHOLDER: &str = "[图片]";
pub(super) const DOCX_TEXTBOX_PLACEHOLDER: &str = "[文本框]";
pub(super) const DOCX_TOC_PLACEHOLDER: &str = "[目录]";

pub(super) fn placeholder_presentation(kind: &str) -> Option<ChunkPresentation> {
    Some(ChunkPresentation {
        bold: false,
        italic: false,
        underline: false,
        href: None,
        protect_kind: Some(kind.to_string()),
        writeback_key: None,
    })
}

pub(super) fn raw_locked_block(
    text: &str,
    kind: &str,
    events: &[Event<'static>],
) -> WritebackBlockTemplate {
    WritebackBlockTemplate::Locked(LockedRegionTemplate {
        text: text.to_string(),
        presentation: placeholder_presentation(kind),
        render: LockedRegionRender::RawEvents(events.to_vec()),
    })
}
```

Wire it in `src-tauri/src/adapters/docx.rs`:

```rust
#[path = "docx/placeholders.rs"]
mod placeholders;
```

- [ ] **Step 4: Teach import parsing to classify block-level `sdt` / drawing-backed objects as locked placeholders**

In `src-tauri/src/adapters/docx/simple.rs`, route top-level blocks away from the generic error path:

```rust
match name.as_slice() {
    b"p" | b"tbl" | b"sectPr" | b"sdt" => {
        block_depth = 1;
        block_name = Some(name.clone());
        block_events.clear();
        block_events.push(Event::Start(e));
    }
    _ => {
        return Err(format!(
            "当前仅支持文章语义相关的 docx：检测到不支持的正文结构 <{}>。",
            tag_name(name.as_slice())
        ))
    }
}
```

Add block classifiers:

```rust
fn contains_local_tag(events: &[Event<'static>], tag: &[u8]) -> bool {
    events.iter().any(|event| match event {
        Event::Start(e) | Event::Empty(e) => local_name(e.name().as_ref()) == tag,
        Event::End(e) => local_name(e.name().as_ref()) == tag,
        _ => false,
    })
}

fn parse_sdt_placeholder_block(events: &[Event<'static>]) -> WritebackBlockTemplate {
    placeholders::raw_locked_block(
        placeholders::DOCX_TOC_PLACEHOLDER,
        "toc",
        events,
    )
}

fn detect_drawing_placeholder(events: &[Event<'static>]) -> (&'static str, &'static str) {
    if contains_local_tag(events, b"txbxContent") {
        (placeholders::DOCX_TEXTBOX_PLACEHOLDER, "textbox")
    } else {
        (placeholders::DOCX_IMAGE_PLACEHOLDER, "image")
    }
}
```

When import sees `w:drawing`, `w:pict`, or `mc:AlternateContent` inside a paragraph/run subtree, push a locked region instead of returning an unsupported-structure error.

- [ ] **Step 5: Re-run the targeted import test**

Run:

```bash
cd src-tauri && cargo test imports_report_template_with_locked_non_article_objects -- --nocapture
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/adapters/docx.rs \
        src-tauri/src/adapters/docx/placeholders.rs \
        src-tauri/src/adapters/docx/simple.rs \
        src-tauri/src/adapters/docx/tests.rs
git commit -m "支持docx模板锁定对象导入"
```

### Task 2: Metadata-Tolerant Paragraph Parsing

**Files:**
- Create: `src-tauri/src/adapters/docx/subtree.rs`
- Modify: `src-tauri/src/adapters/docx/simple.rs`
- Modify: `src-tauri/src/adapters/docx/tests.rs`
- Test: `src-tauri/src/adapters/docx/tests.rs`

- [ ] **Step 1: Write a failing metadata-tolerance regression**

Add a focused unit test with bookmarks, proofing markers, and TOC field markers around visible text:

```rust
#[test]
fn imports_paragraph_with_bookmarks_proofing_and_field_markers() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:bookmarkStart w:id="0" w:name="_Toc1"/>
      <w:r><w:t>标题</w:t></w:r>
      <w:bookmarkEnd w:id="0"/>
      <w:proofErr w:type="spellStart"/>
      <w:r><w:t>正文</w:t></w:r>
      <w:proofErr w:type="spellEnd"/>
      <w:r><w:fldChar w:fldCharType="begin"/></w:r>
      <w:r><w:instrText xml:space="preserve"> TOC \\o "1-3" </w:instrText></w:r>
      <w:r><w:fldChar w:fldCharType="end"/></w:r>
    </w:p>
  </w:body>
</w:document>"#;
    let bytes = build_minimal_docx(xml);
    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");
    let text = regions.iter().map(|item| item.body.as_str()).collect::<String>();

    assert!(text.contains("标题正文"));
}
```

- [ ] **Step 2: Run the targeted test to verify it fails**

Run:

```bash
cd src-tauri && cargo test imports_paragraph_with_bookmarks_proofing_and_field_markers -- --nocapture
```

Expected: FAIL with an unsupported-structure error mentioning `bookmarkStart`, `proofErr`, `fldChar`, or `instrText`.

- [ ] **Step 3: Extract shared subtree helpers**

Create `src-tauri/src/adapters/docx/subtree.rs`:

```rust
use quick_xml::events::Event;

fn local_name(name: &[u8]) -> &[u8] {
    match name.iter().rposition(|b| *b == b':') {
        Some(pos) if pos + 1 < name.len() => &name[pos + 1..],
        _ => name,
    }
}

pub(super) fn contains_local_tag(events: &[Event<'static>], tag: &[u8]) -> bool {
    events.iter().any(|event| match event {
        Event::Start(e) | Event::Empty(e) => local_name(e.name().as_ref()) == tag,
        Event::End(e) => local_name(e.name().as_ref()) == tag,
        _ => false,
    })
}

pub(super) fn is_ignorable_marker(name: &[u8]) -> bool {
    matches!(
        name,
        b"bookmarkStart" | b"bookmarkEnd" | b"proofErr" | b"fldChar" | b"instrText"
    )
}
```

Wire it in `src-tauri/src/adapters/docx.rs`:

```rust
#[path = "docx/subtree.rs"]
mod subtree;
```

- [ ] **Step 4: Skip safe metadata markers instead of rejecting the whole paragraph**

In `src-tauri/src/adapters/docx/simple.rs`, update the paragraph/run match arms:

```rust
match name.as_slice() {
    b"pPr" => *ppr_depth = 1,
    b"r" => { /* existing run setup */ }
    b"hyperlink" => { /* existing hyperlink setup */ }
    b"oMath" | b"oMathPara" => { /* existing formula path */ }
    name if subtree::is_ignorable_marker(name) => {}
    name if is_embedded_object_name(name) => return Err(DOCX_EMBEDDED_OBJECT_ERROR.to_string()),
    _ => { /* existing unsupported error */ }
}
```

Apply the same ignore rule in the writeback template parser:

```rust
match name.as_slice() {
    b"pPr" => {
        index = skip_subtree_events(events, index)?;
        continue;
    }
    name if subtree::is_ignorable_marker(name) => {
        index = next_index;
        continue;
    }
    b"r" => { /* existing run parsing */ }
    b"hyperlink" => { /* existing hyperlink parsing */ }
    b"oMath" | b"oMathPara" => { /* existing formula parsing */ }
    _ => { /* existing unsupported error */ }
}
```

- [ ] **Step 5: Re-run the targeted test**

Run:

```bash
cd src-tauri && cargo test imports_paragraph_with_bookmarks_proofing_and_field_markers -- --nocapture
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/adapters/docx.rs \
        src-tauri/src/adapters/docx/subtree.rs \
        src-tauri/src/adapters/docx/simple.rs \
        src-tauri/src/adapters/docx/tests.rs
git commit -m "放宽docx元数据标记解析"
```

### Task 3: Safe Writeback For Locked Template Objects

**Files:**
- Modify: `src-tauri/src/adapters/docx/simple.rs`
- Modify: `src-tauri/src/adapters/docx/tests.rs`
- Test: `src-tauri/src/adapters/docx/tests.rs`

- [ ] **Step 1: Write the failing fixture round-trip regression**

Add a fixture-based writeback test that edits only article text and asserts locked objects survive:

```rust
#[test]
fn writes_back_report_template_after_editing_article_text() {
    let bytes = load_repo_docx_fixture("04-3 作品报告（大数据应用赛，2025版）模板.docx");
    let source = DocxAdapter::extract_writeback_source_text(&bytes).expect("writeback source");
    let mut regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");

    let target = regions
        .iter_mut()
        .find(|item| !item.skip_rewrite && item.body.contains("作品名称"))
        .expect("editable article region");
    target.body = target.body.replacen("作品名称", "作品标题", 1);

    let rewritten = DocxAdapter::write_updated_regions(&bytes, &source, &regions)
        .expect("write updated regions");
    let rewritten_regions = DocxAdapter::extract_regions(&rewritten, false).expect("re-extract");
    let rewritten_text = rewritten_regions
        .iter()
        .map(|item| item.body.as_str())
        .collect::<String>();

    assert!(rewritten_text.contains("作品标题"));
    assert!(rewritten_regions.iter().any(|r| protect_kind_of(r) == Some("textbox")));
    assert!(rewritten_regions.iter().any(|r| protect_kind_of(r) == Some("toc")));
    assert!(rewritten_regions.iter().any(|r| protect_kind_of(r) == Some("image")));
}
```

- [ ] **Step 2: Run the targeted test to verify it fails**

Run:

```bash
cd src-tauri && cargo test writes_back_report_template_after_editing_article_text -- --nocapture
```

Expected: FAIL in `validate_writeback`, `extract_writeback_paragraph_templates`, or `write_updated_regions` because `sdt`, drawing/textbox content, or field markers are still not preserved in the writeback template.

- [ ] **Step 3: Preserve locked raw blocks and regions through writeback template extraction**

In `src-tauri/src/adapters/docx/simple.rs`, extend writeback block extraction:

```rust
match kind.as_slice() {
    b"p" => WritebackBlockTemplate::Paragraph(
        parse_writeback_paragraph_template(&block_events, hyperlink_targets)?,
    ),
    b"tbl" => parse_table_placeholder_block(&block_events)?,
    b"sdt" => parse_sdt_placeholder_block(&block_events),
    b"sectPr" => parse_section_break_placeholder_block(&block_events)?,
    _ => return Err("解析 docx 写回模板失败：未知正文块类型。".to_string()),
}
```

Add locked region builders for drawing/textbox runs:

```rust
fn parse_object_placeholder_region(events: &[Event<'static>]) -> WritebackRegionTemplate {
    let (text, kind) = if subtree::contains_local_tag(events, b"txbxContent") {
        (placeholders::DOCX_TEXTBOX_PLACEHOLDER, "textbox")
    } else {
        (placeholders::DOCX_IMAGE_PLACEHOLDER, "image")
    };
    WritebackRegionTemplate::Locked(LockedRegionTemplate {
        text: text.to_string(),
        presentation: placeholders::placeholder_presentation(kind),
        render: LockedRegionRender::RawEvents(events.to_vec()),
    })
}
```

Then allow `drawing`, `pict`, and `AlternateContent` in both import and writeback run/paragraph parsing to route into that locked region instead of throwing.

- [ ] **Step 4: Re-run the targeted writeback test**

Run:

```bash
cd src-tauri && cargo test writes_back_report_template_after_editing_article_text -- --nocapture
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/adapters/docx/simple.rs \
        src-tauri/src/adapters/docx/tests.rs
git commit -m "支持docx模板安全写回"
```

### Task 4: Chunk Pipeline And Manual Test Coverage

**Files:**
- Modify: `src-tauri/src/adapters/docx/tests.rs`
- Modify: `docs/testing-guide.md`
- Test: `src-tauri/src/adapters/docx/tests.rs`

- [ ] **Step 1: Write a failing chunk-pipeline regression**

Add a test that proves locked template objects do not become editable rewrite chunks:

```rust
#[test]
fn keeps_template_placeholders_out_of_editable_chunks() {
    let bytes = load_repo_docx_fixture("04-3 作品报告（大数据应用赛，2025版）模板.docx");
    let regions = DocxAdapter::extract_regions(&bytes, false).expect("extract regions");
    let chunks = rewrite::segment_regions(regions, ChunkPreset::Paragraph);

    assert!(chunks.iter().any(|chunk| !chunk.skip_rewrite && chunk.text.contains("作品名称")));
    assert!(chunks.iter().any(|chunk| chunk.skip_rewrite && chunk.text == "[文本框]"));
    assert!(chunks.iter().any(|chunk| chunk.skip_rewrite && chunk.text == "[目录]"));
    assert!(chunks.iter().any(|chunk| chunk.skip_rewrite && chunk.text == "[图片]"));
}
```

- [ ] **Step 2: Run the targeted test to verify it fails only if placeholder wiring is incomplete**

Run:

```bash
cd src-tauri && cargo test keeps_template_placeholders_out_of_editable_chunks -- --nocapture
```

Expected: if placeholder text or `skip_rewrite` flags are not wired correctly yet, FAIL with missing placeholder assertion.

- [ ] **Step 3: Finish placeholder text + `skip_rewrite` wiring**

Make sure import pushes locked placeholder regions using the existing `skip_rewrite: true` path:

```rust
push_import_region(
    regions,
    placeholders::DOCX_TEXTBOX_PLACEHOLDER.to_string(),
    true,
    placeholders::placeholder_presentation("textbox"),
);
```

Use the same pattern for `toc` and `image`.

- [ ] **Step 4: Re-run the targeted chunk test**

Run:

```bash
cd src-tauri && cargo test keeps_template_placeholders_out_of_editable_chunks -- --nocapture
```

Expected: PASS.

- [ ] **Step 5: Update the manual testing guide**

Append a dedicated docx-template scenario to `docs/testing-guide.md`:

```md
### 6.3.1 比赛模板 docx

打开 `testdoc/04-3 作品报告（大数据应用赛，2025版）模板.docx`。

重点检查：

- 文档可以成功导入，不会因文本框、目录、图片或表格直接报错。
- 正文相关文字可阅读、可切块、可进入正常处理流程。
- 文本框、目录、图片、表格显示为锁定占位符，不参与 AI 改写。
- 对正文做少量修改后，`finalize` 可成功写回原 `.docx`。
- 写回后重新打开，占位结构仍然存在，正文修改保留。
```

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/adapters/docx/tests.rs docs/testing-guide.md
git commit -m "补充docx模板回归测试说明"
```

### Task 5: Full Verification

**Files:**
- Modify: none
- Test: `src-tauri/src/adapters/docx/tests.rs`, `docs/testing-guide.md`

- [ ] **Step 1: Run focused docx regressions**

Run:

```bash
cd src-tauri && cargo test docx -- --nocapture
```

Expected: all docx tests PASS, including the new template-import, metadata-tolerance, chunk-pipeline, and template-writeback regressions.

- [ ] **Step 2: Run rewrite regressions**

Run:

```bash
cd src-tauri && cargo test rewrite -- --nocapture
```

Expected: PASS with no regression in chunking behavior outside docx.

- [ ] **Step 3: Run frontend typecheck**

Run:

```bash
pnpm run typecheck
```

Expected: PASS.

- [ ] **Step 4: Manual smoke test the real template**

Run:

```bash
pnpm run tauri:dev
```

Manual path:

1. Open `testdoc/04-3 作品报告（大数据应用赛，2025版）模板.docx`
2. Confirm placeholder blocks appear for textbox, TOC, image, and table
3. Apply a small正文 edit
4. Trigger finalize
5. Re-open the file and confirm正文 edit remains while placeholders still render as locked structures

- [ ] **Step 5: Commit verification-only follow-up if manual smoke uncovered fixture drift**

```bash
git status --short
```

Expected: no unreviewed changes after automated and manual verification. If clean, do not create an extra commit.

## Self-Review

### Spec Coverage

- Import success for the report template: Task 1
- Locked placeholders for image/textbox/toc/table/section/formula: Tasks 1 and 3
- Ignoring bookmarks/proofing/field metadata: Task 2
- Safe writeback with locked objects preserved: Task 3
- Editable正文 only, placeholders excluded from rewrite chunks: Task 4
- Fixture-based regression and manual validation: Tasks 1, 3, 4, and 5

### Placeholder Scan

- No `TODO`, `TBD`, or “similar to” instructions remain.
- Every code-changing step includes concrete Rust or Markdown content.
- Every verification step has an exact command and expected outcome.

### Type Consistency

- Placeholder kinds are consistently named `image`, `textbox`, `toc`, `table`, `formula`, `page-break`, and `section-break`.
- Locked placeholder text stays `[图片]`, `[文本框]`, `[目录]`, `[表格]`, `[分页符]`, and `[分节符]`.
- The plan keeps `DocxAdapter::extract_regions`, `DocxAdapter::extract_writeback_source_text`, and `DocxAdapter::write_updated_regions` as the existing public test surface.
