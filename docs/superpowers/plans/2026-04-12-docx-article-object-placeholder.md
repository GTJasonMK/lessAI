# Docx Article Object Placeholder Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Support visible locked placeholders for article-relevant docx chart/shape/group-shape objects, while keeping the rule that every visible AI-noneditable placeholder must still round-trip through safe writeback.

**Architecture:** Keep all changes inside the existing `DocxAdapter` path. Extend object classification in `src-tauri/src/adapters/docx/simple.rs`, add new placeholder constants in `src-tauri/src/adapters/docx/placeholders.rs`, and reuse the existing raw-event locked-region writeback model so import, chunking, merged regions, and writeback stay on one shared pipeline.

**Tech Stack:** Rust, `quick-xml`, `zip`, existing `DocxAdapter` import/writeback model, Rust unit tests in `src-tauri/src/adapters/docx/tests.rs`, Markdown docs in `docs/testing-guide.md`

---

## File Map

- Modify: `src-tauri/src/adapters/docx/placeholders.rs`
  Purpose: define the new `[图表]`, `[图形]`, and `[组合图形]` placeholder text and keep `protect_kind` naming centralized.

- Modify: `src-tauri/src/adapters/docx/simple.rs`
  Purpose: extend inline/writeback object classification, keep unsupported unknown objects as explicit errors, and reuse the existing locked-region raw-event writeback path.

- Modify: `src-tauri/src/adapters/docx/tests.rs`
  Purpose: add import, writeback round-trip, and mutation-rejection regressions for chart/shape/group-shape placeholders and unknown-object rejection.

- Modify: `docs/testing-guide.md`
  Purpose: document the new manual regression expectations for placeholder-backed article objects.

- No frontend file changes expected
  Reason: placeholder text is already rendered from region content, and `protect_kind` is already carried through the shared chunk/session model.

### Task 1: Import Classification For Article Objects

**Files:**
- Modify: `src-tauri/src/adapters/docx/placeholders.rs`
- Modify: `src-tauri/src/adapters/docx/simple.rs`
- Modify: `src-tauri/src/adapters/docx/tests.rs`
- Test: `src-tauri/src/adapters/docx/tests.rs`

- [ ] **Step 1: Write the failing import tests**

Add these regressions in `src-tauri/src/adapters/docx/tests.rs` near the existing placeholder tests:

```rust
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

    assert!(regions.iter().any(|r| r.body == "[图表]" && protect_kind_of(r) == Some("chart")));
    assert!(regions.iter().any(|r| r.body == "[图形]" && protect_kind_of(r) == Some("shape")));
    assert!(regions
        .iter()
        .any(|r| r.body == "[组合图形]" && protect_kind_of(r) == Some("group-shape")));
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
```

- [ ] **Step 2: Run the targeted tests to verify they fail**

Run:

```bash
cd src-tauri && timeout 60s cargo test imports_chart_shape_and_group_shape_as_locked_placeholders -- --nocapture
cd src-tauri && timeout 60s cargo test rejects_unknown_article_object_that_cannot_be_classified_safely -- --nocapture
```

Expected: both tests FAIL because current classification only recognizes image/textbox-like drawing objects and falls back to an unsupported-structure error.

- [ ] **Step 3: Add the new placeholder constants**

Update `src-tauri/src/adapters/docx/placeholders.rs`:

```rust
pub(super) const DOCX_CHART_PLACEHOLDER: &str = "[图表]";
pub(super) const DOCX_SHAPE_PLACEHOLDER: &str = "[图形]";
pub(super) const DOCX_GROUP_SHAPE_PLACEHOLDER: &str = "[组合图形]";
```

Keep them next to the existing placeholder constants so all placeholder text stays centralized in one file.

- [ ] **Step 4: Replace the tuple-only classifier with a fallible shared classifier**

In `src-tauri/src/adapters/docx/simple.rs`, replace the current `locked_inline_placeholder(...) -> (&str, &str)` helper with an explicit classifier:

```rust
fn classify_locked_object_placeholder(
    events: &[Event<'static>],
) -> Result<(&'static str, &'static str), String> {
    if contains_local_tag(events, b"txbxContent") || contains_local_tag(events, b"textbox") {
        return Ok((placeholders::DOCX_TEXTBOX_PLACEHOLDER, "textbox"));
    }
    if contains_local_tag(events, b"pic") {
        return Ok((placeholders::DOCX_IMAGE_PLACEHOLDER, "image"));
    }
    if contains_local_tag(events, b"chart") {
        return Ok((placeholders::DOCX_CHART_PLACEHOLDER, "chart"));
    }
    if contains_local_tag(events, b"wgp") || contains_local_tag(events, b"grpSp") {
        return Ok((placeholders::DOCX_GROUP_SHAPE_PLACEHOLDER, "group-shape"));
    }
    if contains_local_tag(events, b"wsp")
        || contains_local_tag(events, b"sp")
        || contains_local_tag(events, b"cxnSp")
        || contains_local_tag(events, b"graphicFrame")
    {
        return Ok((placeholders::DOCX_SHAPE_PLACEHOLDER, "shape"));
    }
    Err("当前仅支持文章语义相关的 docx：无法归类正文中的图形对象，无法安全导入。".to_string())
}
```

Update callers to propagate errors instead of silently treating unknown objects as images:

```rust
fn push_locked_inline_object_region(
    regions: &mut Vec<TextRegion>,
    events: &[Event<'static>],
) -> Result<(), String> {
    let (text, kind) = classify_locked_object_placeholder(events)?;
    push_import_region(
        regions,
        text.to_string(),
        true,
        placeholders::placeholder_presentation(kind),
    );
    Ok(())
}
```

And inside `try_capture_locked_inline_object_event(...)`:

```rust
Event::Empty(_) => push_locked_inline_object_region(regions, std::slice::from_ref(event))?,
```

And when the captured subtree closes:

```rust
push_locked_inline_object_region(regions, locked_inline_events)?;
```

- [ ] **Step 5: Re-run the import tests to verify they pass**

Run:

```bash
cd src-tauri && timeout 60s cargo test imports_chart_shape_and_group_shape_as_locked_placeholders -- --nocapture
cd src-tauri && timeout 60s cargo test rejects_unknown_article_object_that_cannot_be_classified_safely -- --nocapture
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/adapters/docx/placeholders.rs \
        src-tauri/src/adapters/docx/simple.rs \
        src-tauri/src/adapters/docx/tests.rs
git commit -m "支持docx图形对象占位导入"
```

### Task 2: Writeback Round-Trip And Placeholder Mutation Rejection

**Files:**
- Modify: `src-tauri/src/adapters/docx/simple.rs`
- Modify: `src-tauri/src/adapters/docx/tests.rs`
- Test: `src-tauri/src/adapters/docx/tests.rs`

- [ ] **Step 1: Write the failing writeback regressions**

Add these tests in `src-tauri/src/adapters/docx/tests.rs`:

```rust
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
    let rewritten = DocxAdapter::write_updated_regions(&bytes, &source, &regions)
        .expect("write updated regions");

    assert_eq!(read_docx_entry(&rewritten, "word/document.xml"), read_docx_entry(&bytes, "word/document.xml"));
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
    let chart = regions
        .iter_mut()
        .find(|region| protect_kind_of(region) == Some("chart"))
        .expect("chart placeholder");
    chart.body = "[已改坏图表]".to_string();

    let error = DocxAdapter::write_updated_regions(&bytes, &source, &regions)
        .expect_err("expected locked placeholder rejection");

    assert!(error.contains("锁定内容") || error.contains("占位符") || error.contains("锁定区"));
}
```

- [ ] **Step 2: Run the targeted tests to verify they fail**

Run:

```bash
cd src-tauri && timeout 60s cargo test roundtrips_chart_shape_and_group_shape_placeholders_through_writeback -- --nocapture
cd src-tauri && timeout 60s cargo test rejects_writeback_when_chart_shape_or_group_placeholder_text_changes -- --nocapture
```

Expected: the round-trip test FAILS because writeback extraction still hardcodes image/textbox placeholder classification, while the mutation test may either fail to find a `chart` placeholder or fail earlier in extraction.

- [ ] **Step 3: Reuse the same classifier in writeback region extraction**

In `src-tauri/src/adapters/docx/simple.rs`, replace writeback-side tuple logic with the shared classifier. Update these branches:

```rust
name if is_locked_inline_object_name(name) => {
    let (text, kind) = classify_locked_object_placeholder(&child_events)?;
    regions.push(placeholders::raw_locked_region(text, kind, &child_events));
}
```

Apply the same change in:

- `parse_writeback_paragraph_template(...)`
- `parse_writeback_hyperlink_regions(...)`
- any helper that currently calls `locked_inline_placeholder(...)`

Then delete the old `locked_inline_placeholder(...)` helper entirely so import and writeback cannot drift again.

- [ ] **Step 4: Verify that locked-placeholder mutation still rejects edits through the existing validator**

Do not add a new writeback branch. Keep the existing validator path in `validate_updated_locked_region(...)` and confirm it still rejects placeholder text edits once the new `protect_kind`s reach writeback extraction.

The expected code path remains:

```rust
if expected_body != updated.body {
    return Err("写回内容改动了锁定内容（例如公式、分页符或占位符），已拒绝写回。".to_string());
}
```

This task is complete only if chart/shape/group-shape placeholders are covered by the same validator without a special-case exemption.

- [ ] **Step 5: Re-run the targeted tests plus the existing template regressions**

Run:

```bash
cd src-tauri && timeout 60s cargo test roundtrips_chart_shape_and_group_shape_placeholders_through_writeback -- --nocapture
cd src-tauri && timeout 60s cargo test rejects_writeback_when_chart_shape_or_group_placeholder_text_changes -- --nocapture
cd src-tauri && timeout 60s cargo test roundtrips_report_template_writeback_regions_with_locked_non_article_objects -- --nocapture
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/adapters/docx/simple.rs \
        src-tauri/src/adapters/docx/tests.rs
git commit -m "完善docx图形对象占位写回"
```

### Task 3: Manual Regression Guide And Final Verification

**Files:**
- Modify: `docs/testing-guide.md`
- Test: `docs/testing-guide.md`

- [ ] **Step 1: Update the manual docx regression guide**

In `docs/testing-guide.md`, extend the template专项回归 section under DOCX with these bullets:

```md
- 图表显示为 `[图表]` 锁定占位。
- 普通图形或 SmartArt 类对象显示为 `[图形]` 锁定占位。
- 组合图形显示为 `[组合图形]` 锁定占位。
- 这些占位符在工作区可见，但不会进入 AI 改写，也不能在编辑器里被改后安全写回。
- 无法归类且无法安全抓取的未知图形对象，应直接导入失败，不得伪装成正文或错误占位符。
```

- [ ] **Step 2: Run final verification**

Run:

```bash
pnpm run typecheck
cd src-tauri && cargo fmt --check
cd src-tauri && timeout 60s cargo test docx -- --nocapture
git diff --check
```

Expected:

- `pnpm run typecheck` passes
- `cargo fmt --check` passes
- `cargo test docx` passes, including the new chart/shape/group-shape coverage
- `git diff --check` reports no whitespace or conflict-marker issues

- [ ] **Step 3: Commit**

```bash
git add docs/testing-guide.md
git commit -m "补充docx对象占位测试说明"
```
