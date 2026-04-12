# Docx Article Semantic Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expand `.docx` support from simple plain paragraphs to an article-oriented subset that keeps common article semantics readable, rewriteable, and safely writable back.

**Architecture:** Split the current monolithic `docx.rs` into focused OOXML modules. Parse `word/document.xml` plus relationships into an internal article-semantic model, flatten that model into rewrite-safe `TextRegion`s and chunk presentation metadata, and persist a backend-only writeback plan with XML anchors. Reuse the existing `segment_regions` pipeline and a shared lock-placeholder helper for locked content validation instead of creating a second docx rewrite engine.

**Tech Stack:** Rust, Tauri, `quick-xml`, `zip`, React, TypeScript

---

## Planned File Map

- Create: `src-tauri/src/adapters/docx/mod.rs` - public `DocxAdapter` entry points and high-level orchestration.
- Create: `src-tauri/src/adapters/docx/archive.rs` - read/replace OOXML zip parts and relationship files.
- Create: `src-tauri/src/adapters/docx/model.rs` - article block/inline model, marks, placeholders, writeback anchors, import result.
- Create: `src-tauri/src/adapters/docx/parse.rs` - strict OOXML parser and hard-reject checks.
- Create: `src-tauri/src/adapters/docx/flatten.rs` - semantic model to `TextRegion` conversion and editor capability flags.
- Create: `src-tauri/src/adapters/docx/writeback.rs` - anchor validation and XML text replacement.
- Create: `src-tauri/src/adapters/docx/tests.rs` - article-semantic import/writeback regression coverage.
- Create: `src-tauri/src/rewrite/llm/locked_placeholders.rs` - shared lock/mask/restore helper used by markdown, tex, and docx writeback validation.
- Modify: `src-tauri/src/adapters/mod.rs` - extend `TextRegion` metadata.
- Modify: `src-tauri/src/rewrite/types.rs` - carry source-region identity and presentation through segmentation.
- Modify: `src-tauri/src/rewrite/segment/regions.rs` - preserve region metadata on split chunks.
- Modify: `src-tauri/src/models.rs` - add chunk presentation and docx plain-text editor capability fields.
- Modify: `src-tauri/src/documents.rs` - surface docx import result and persist sidecar writeback plans.
- Modify: `src-tauri/src/storage.rs` - save/load/delete docx writeback plan sidecars.
- Modify: `src-tauri/src/commands/session.rs` - session rebuild + plain-text editor gating for rich docx.
- Modify: `src-tauri/src/commands/export.rs` - finalize rich docx from session chunk data instead of paragraph-only text.
- Modify: `src-tauri/src/rewrite/llm/markdown.rs`
- Modify: `src-tauri/src/rewrite/llm/tex.rs`
- Modify: `src/lib/types.ts` - mirror new session/chunk metadata.
- Create: `src/lib/docxPresentation.tsx` - render bold/italic/underline/link/protected docx chunks.
- Modify: `src/stages/workbench/document/DocumentFlow.tsx`
- Modify: `src/stages/workbench/review/SuggestionReviewPane.tsx`
- Modify: `src/stages/workbench/review/EditorReviewPane.tsx`
- Modify: `src/stages/workbench/DocumentPanel.tsx`
- Modify: `src/app/hooks/useDocumentActions.ts`
- Modify: `src/app/hooks/useDocumentFinalizeActions.ts`

### Task 1: Split the docx adapter and pin down the supported subset with failing tests

**Files:**
- Create: `src-tauri/src/adapters/docx/mod.rs`
- Create: `src-tauri/src/adapters/docx/model.rs`
- Create: `src-tauri/src/adapters/docx/tests.rs`
- Modify: `src-tauri/src/adapters/mod.rs`

- [ ] **Step 1: Move the current `docx.rs` API surface behind a module folder without changing callers**

```rust
// src-tauri/src/adapters/docx/mod.rs
mod archive;
mod flatten;
mod model;
mod parse;
mod tests;
mod writeback;

pub struct DocxAdapter;

impl DocxAdapter {
    pub fn extract_regions(docx_bytes: &[u8], rewrite_headings: bool) -> Result<DocxImport, String> {
        let package = archive::read_package(docx_bytes)?;
        let article = parse::parse_article_document(&package)?;
        flatten::build_import(article, rewrite_headings)
    }
}
```

- [ ] **Step 2: Write failing parser tests for the approved article subset**

```rust
#[test]
fn imports_lists_links_formulas_and_placeholders() {
    let bytes = fixture_docx("article_semantic_sample.xml");
    let imported = DocxAdapter::extract_regions(&bytes, false).expect("import docx");

    assert!(imported.source_text.contains("[图片：图1 实验装置]"));
    assert!(imported.source_text.contains("[表格：表2 实验结果]"));
    assert!(imported.source_text.contains("[分页符]"));
    assert!(imported.regions.iter().any(|r| r.skip_rewrite && r.body.contains("E=mc^2")));
    assert!(imported.regions.iter().any(|r| !r.skip_rewrite && r.presentation.as_ref().is_some_and(|p| p.bold)));
}

#[test]
fn rejects_tracked_changes_comments_and_footnotes() {
    let bytes = fixture_docx("reject_review_markup.xml");
    let error = DocxAdapter::extract_regions(&bytes, false).expect_err("must reject");

    assert!(error.contains("修订") || error.contains("批注") || error.contains("脚注"));
}

#[test]
fn ignores_headers_and_footers_but_rejects_embedded_office_objects() {
    let bytes = fixture_docx("reject_embedded_object.xml");
    let error = DocxAdapter::extract_regions(&bytes, false).expect_err("must reject");

    assert!(error.contains("嵌入对象") || error.contains("图表") || error.contains("SmartArt"));
}
```

- [ ] **Step 3: Run the focused test command and confirm the failures are about missing article-semantic support**

Run: `cmd.exe /c "cd /d E:\\Code\\LessAI\\src-tauri && cargo test docx -- --nocapture"`

Expected: FAIL in the new docx tests with messages indicating missing list/hyperlink/formula/placeholder parsing or missing hard-reject checks.

- [ ] **Step 4: Commit the adapter split and failing tests**

```bash
git add src-tauri/src/adapters/mod.rs src-tauri/src/adapters/docx
git commit -m "拆分 docx 语义适配器骨架"
```

### Task 2: Parse OOXML into an article-semantic model and flatten it into safe regions

**Files:**
- Create: `src-tauri/src/adapters/docx/archive.rs`
- Create: `src-tauri/src/adapters/docx/parse.rs`
- Create: `src-tauri/src/adapters/docx/flatten.rs`
- Modify: `src-tauri/src/adapters/docx/model.rs`

- [ ] **Step 1: Define the internal semantic model and writeback anchors**

```rust
#[derive(Debug, Clone)]
pub struct DocxImport {
    pub source_text: String,
    pub regions: Vec<TextRegion>,
    pub plain_text_editor_safe: bool,
    pub plain_text_editor_reason: Option<String>,
    pub writeback_plan: DocxWritebackPlan,
}

#[derive(Debug, Clone)]
pub enum DocxInline {
    EditableText(EditableSpan),
    LockedText(LockedSpan),
    Placeholder(PlaceholderSpan),
}

#[derive(Debug, Clone)]
pub struct EditableSpan {
    pub region_id: String,
    pub text: String,
    pub marks: InlineMarks,
    pub hyperlink_target: Option<String>,
    pub text_anchors: Vec<TextAnchor>,
}
```

- [ ] **Step 2: Implement strict parsing for supported article content and explicit rejection for unsupported Word features**

```rust
match local_name(name) {
    b"ins" | b"del" => return Err("当前不支持带修订痕迹的 docx，请先接受或拒绝所有修订。".to_string()),
    b"commentRangeStart" | b"commentReference" => return Err("当前不支持带批注的 docx。".to_string()),
    b"footnoteReference" | b"endnoteReference" => return Err("当前不支持脚注或尾注。".to_string()),
    b"txbxContent" => inline_nodes.extend(parse_textbox_article_text(node, rels, captions)?),
    b"drawing" => inline_nodes.push(parse_drawing_as_placeholder(node, captions)?),
    b"tbl" => block_nodes.push(parse_table_as_placeholder(node, captions)?),
    b"fldSimple" => inline_nodes.push(parse_field_as_placeholder(node)?),
    b"hyperlink" => inline_nodes.push(parse_hyperlink(node, rels)?),
    b"oMath" | b"oMathPara" => inline_nodes.push(parse_formula_as_locked(node)?),
    b"br" if is_page_break(node) => inline_nodes.push(PlaceholderSpan::page_break()),
    b"sectPr" => block_nodes.push(DocxBlock::Placeholder(PlaceholderSpan::section_break())),
    b"hdr" | b"ftr" => {}
    b"object" | b"OLEObject" | b"chart" | b"relIds" => {
        return Err("当前不支持图表、SmartArt 或嵌入 Office 对象。".to_string())
    }
    _ => { /* supported paragraph/list/run traversal */ }
}
```

- [ ] **Step 3: Flatten the semantic model into existing `TextRegion`s while preserving safe boundaries**

```rust
fn push_editable_region(out: &mut Vec<TextRegion>, span: &EditableSpan) {
    out.push(TextRegion {
        body: span.text.clone(),
        skip_rewrite: false,
        source_region_id: Some(span.region_id.clone()),
        presentation: Some(ChunkPresentation::from_marks(&span.marks, span.hyperlink_target.clone())),
    });
}

fn push_locked_region(out: &mut Vec<TextRegion>, span: &LockedSpan) {
    out.push(TextRegion {
        body: span.display_text.clone(),
        skip_rewrite: true,
        source_region_id: Some(span.region_id.clone()),
        presentation: Some(ChunkPresentation::locked(span.label.clone())),
    });
}
```

- [ ] **Step 4: Mark rich docx as unsafe for the current plain-text editor whenever span boundaries cannot be reconstructed from raw text**

```rust
fn plain_text_editor_capability(blocks: &[DocxBlock]) -> (bool, Option<String>) {
    let requires_semantic_boundaries = blocks.iter().any(DocxBlock::has_multiple_editable_spans);
    if requires_semantic_boundaries {
        return (
            false,
            Some("该 docx 含行内样式、链接或受保护结构，暂不支持在纯文本编辑器中直接覆写。".to_string()),
        );
    }
    (true, None)
}
```

- [ ] **Step 5: Run the focused docx tests again and confirm import coverage is now green**

Run: `cmd.exe /c "cd /d E:\\Code\\LessAI\\src-tauri && cargo test docx -- --nocapture"`

Expected: PASS for the new parser/flatten tests; remaining failures, if any, should be writeback or session integration only.

- [ ] **Step 6: Commit the parser and flattener**

```bash
git add src-tauri/src/adapters/docx
git commit -m "实现 docx 文章语义解析"
```

### Task 3: Thread region identity and presentation through chunks, sessions, and storage

**Files:**
- Modify: `src-tauri/src/adapters/mod.rs`
- Modify: `src-tauri/src/rewrite/types.rs`
- Modify: `src-tauri/src/rewrite/segment/regions.rs`
- Modify: `src-tauri/src/models.rs`
- Modify: `src-tauri/src/documents.rs`
- Modify: `src-tauri/src/storage.rs`
- Modify: `src/lib/types.ts`

- [ ] **Step 1: Extend region/chunk/session types so writeback and UI can keep semantic identity**

```rust
pub struct TextRegion {
    pub body: String,
    pub skip_rewrite: bool,
    pub source_region_id: Option<String>,
    pub presentation: Option<ChunkPresentation>,
}

pub struct SegmentedChunk {
    pub text: String,
    pub separator_after: String,
    pub skip_rewrite: bool,
    pub source_region_id: Option<String>,
    pub presentation: Option<ChunkPresentation>,
}
```

- [ ] **Step 2: Propagate metadata in `segment_regions` so every chunk produced from a docx span still points back to the original semantic region**

```rust
chunks.push(SegmentedChunk {
    text: body,
    separator_after: trailing_ws,
    skip_rewrite,
    source_region_id: region.source_region_id.clone(),
    presentation: region.presentation.clone(),
});
```

- [ ] **Step 3: Persist backend-only docx writeback plans in a sidecar file instead of sending XML anchors to the frontend**

```rust
const DOCX_PLANS_DIR: &str = "docx-writeback";

pub fn save_docx_writeback_plan(
    app: &AppHandle,
    session_id: &str,
    plan: &DocxWritebackPlan,
) -> Result<(), String> {
    let path = sessions_root(app)?.join(DOCX_PLANS_DIR).join(format!("{session_id}.json"));
    write_json(&path, plan)
}
```

- [ ] **Step 4: Mirror the new capability and presentation fields in TypeScript**

```ts
export interface ChunkPresentation {
  bold: boolean;
  italic: boolean;
  underline: boolean;
  href: string | null;
  protectedKind: string | null;
  label: string | null;
}

export interface ChunkTask {
  index: number;
  sourceText: string;
  separatorAfter: string;
  skipRewrite: boolean;
  sourceRegionId: string | null;
  presentation: ChunkPresentation | null;
  status: ChunkStatus;
  errorMessage: string | null;
}
```

- [ ] **Step 5: Run typechecking and targeted Rust tests**

Run: `pnpm run typecheck`

Expected: PASS after TS type updates.

Run: `cmd.exe /c "cd /d E:\\Code\\LessAI\\src-tauri && cargo test docx -- --nocapture"`

Expected: PASS for import tests and any new storage/session serialization coverage.

- [ ] **Step 6: Commit the metadata threading and storage sidecar**

```bash
git add src-tauri/src/adapters/mod.rs src-tauri/src/rewrite/types.rs src-tauri/src/rewrite/segment/regions.rs src-tauri/src/models.rs src-tauri/src/documents.rs src-tauri/src/storage.rs src/lib/types.ts
git commit -m "串联 docx 语义元数据"
```

### Task 4: Implement safe writeback for rich docx and keep editor failures explicit

**Files:**
- Create: `src-tauri/src/adapters/docx/writeback.rs`
- Create: `src-tauri/src/rewrite/llm/locked_placeholders.rs`
- Modify: `src-tauri/src/adapters/docx/mod.rs`
- Modify: `src-tauri/src/rewrite/llm/markdown.rs`
- Modify: `src-tauri/src/rewrite/llm/tex.rs`
- Modify: `src-tauri/src/commands/export.rs`
- Modify: `src-tauri/src/commands/session.rs`
- Modify: `src-tauri/src/documents.rs`

- [ ] **Step 1: Extract the existing placeholder masking logic into a shared helper used by markdown, tex, and docx writeback validation**

```rust
pub fn mask_locked_regions(regions: &[TextRegion]) -> (String, Vec<(String, String)>) {
    let mut seq = 1usize;
    let mut masked = String::new();
    let mut placeholders = Vec::new();

    for region in regions {
        if region.skip_rewrite {
            let token = format!("⟦LESSAI_LOCK_{seq}⟧");
            seq += 1;
            placeholders.push((token.clone(), region.body.clone()));
            masked.push_str(&token);
        } else {
            masked.push_str(&region.body);
        }
    }

    (masked, placeholders)
}
```

- [ ] **Step 2: Write docx writeback against region IDs and XML anchors, not by replacing whole paragraphs with plain runs**

```rust
pub fn write_updated_session(
    docx_bytes: &[u8],
    expected_source_text: &str,
    session_chunks: &[ChunkTask],
    plan: &DocxWritebackPlan,
) -> Result<Vec<u8>, String> {
    plan.validate_source_text(expected_source_text)?;
    let region_updates = collect_region_updates(session_chunks)?;
    let updated_xml = rewrite_document_xml_with_anchors(docx_bytes, plan, &region_updates)?;
    archive::replace_document_xml(docx_bytes, &updated_xml)
}
```

- [ ] **Step 3: Finalize rich docx from session chunks, but reject plain-text editor saves when the imported plan marked them unsafe**

```rust
if is_docx_path(&target) && !existing.can_plain_text_edit {
    return Err(
        existing.plain_text_edit_reason
            .clone()
            .unwrap_or_else(|| "该 docx 暂不支持在纯文本编辑器中直接覆写。".to_string()),
    );
}
```

- [ ] **Step 4: Keep all hard failures explicit**

```rust
if !placeholders_preserved_in_order(&candidate, &placeholders) {
    return Err("docx 写回失败：受保护内容的位置已变化，无法安全映射回原始 XML。".to_string());
}

if region_updates.keys().any(|id| !plan.editable_region_ids.contains(id)) {
    return Err("docx 写回失败：检测到未知可编辑区域。".to_string());
}
```

- [ ] **Step 5: Add regression tests for hyperlink text updates, formula protection, placeholder round-trip, and ambiguous anchor failure**

```rust
#[test]
fn finalizes_hyperlink_text_without_touching_url() { /* sourceRegionId + rel target asserts */ }

#[test]
fn rejects_writeback_when_locked_formula_moves() { /* placeholder order failure */ }
```

- [ ] **Step 6: Run targeted backend verification**

Run: `cmd.exe /c "cd /d E:\\Code\\LessAI\\src-tauri && cargo test docx -- --nocapture"`

Expected: PASS for hyperlink/formula/placeholder writeback coverage.

- [ ] **Step 7: Commit the writeback implementation**

```bash
git add src-tauri/src/adapters/docx src-tauri/src/rewrite/llm/locked_placeholders.rs src-tauri/src/rewrite/llm/markdown.rs src-tauri/src/rewrite/llm/tex.rs src-tauri/src/commands/export.rs src-tauri/src/commands/session.rs src-tauri/src/documents.rs
git commit -m "实现 docx 安全写回"
```

### Task 5: Render docx chunk semantics in the workbench and expose editor limits clearly

**Files:**
- Create: `src/lib/docxPresentation.tsx`
- Modify: `src/stages/workbench/document/DocumentFlow.tsx`
- Modify: `src/stages/workbench/review/SuggestionReviewPane.tsx`
- Modify: `src/stages/workbench/review/EditorReviewPane.tsx`
- Modify: `src/stages/workbench/DocumentPanel.tsx`
- Modify: `src/app/hooks/useDocumentActions.ts`
- Modify: `src/app/hooks/useDocumentFinalizeActions.ts`

- [ ] **Step 1: Add a dedicated renderer for docx chunk presentation instead of overloading the existing markdown/tex protected-text parser**

```tsx
export function renderDocxChunkText(
  text: string,
  presentation: ChunkPresentation | null,
  keyPrefix: string
): ReactNode {
  let node: ReactNode = text;
  if (presentation?.href) node = <span className="docx-link">{node}</span>;
  if (presentation?.underline) node = <span className="docx-underline">{node}</span>;
  if (presentation?.italic) node = <em>{node}</em>;
  if (presentation?.bold) node = <strong>{node}</strong>;
  return presentation?.protectedKind ? <span className="inline-protected">{node}</span> : node;
}
```

- [ ] **Step 2: Use chunk presentation when rendering source/final/diff panes**

```tsx
const renderChunkValue = (value: string, chunk: ChunkTask, key: string) => {
  if (documentFormat !== "docx") return renderInlineProtectedText(value, documentFormat, key);
  return renderDocxChunkText(value, chunk.presentation, key);
};
```

- [ ] **Step 3: Surface explicit notices when a rich docx cannot enter the plain-text editor**

```ts
if (isDocxPath(session.documentPath) && !session.canPlainTextEdit) {
  showNotice("warning", session.plainTextEditReason ?? "该 docx 暂不支持纯文本编辑。");
  return;
}
```

- [ ] **Step 4: Add a lightweight UI regression for docx presentation classes**

```js
assert(css.includes(".docx-link"));
assert(css.includes(".docx-underline"));
assert(css.includes(".inline-protected"));
```

- [ ] **Step 5: Run frontend verification**

Run: `pnpm run typecheck`

Expected: PASS.

Run: `node scripts/ui-regression.test.mjs`

Expected: PASS with the new docx presentation selectors included.

- [ ] **Step 6: Commit the UI updates**

```bash
git add src/lib/docxPresentation.tsx src/stages/workbench/document/DocumentFlow.tsx src/stages/workbench/review/SuggestionReviewPane.tsx src/stages/workbench/review/EditorReviewPane.tsx src/stages/workbench/DocumentPanel.tsx src/app/hooks/useDocumentActions.ts src/app/hooks/useDocumentFinalizeActions.ts scripts/ui-regression.test.mjs
git commit -m "渲染 docx 行内语义"
```

### Task 6: End-to-end verification and cleanup

**Files:**
- Modify: `src-tauri/src/adapters/docx/tests.rs` if any integration cases are still missing
- Modify: `docs/superpowers/specs/2026-04-09-docx-article-semantic-support-design.md` only if the implemented editor boundary needs to be recorded explicitly

- [ ] **Step 1: Run the full backend docx and general regression suite**

Run: `cmd.exe /c "cd /d E:\\Code\\LessAI\\src-tauri && cargo test -- --nocapture"`

Expected: PASS.

- [ ] **Step 2: Run frontend static verification**

Run: `pnpm run typecheck`

Expected: PASS.

Run: `node scripts/ui-regression.test.mjs`

Expected: PASS.

- [ ] **Step 3: Perform a manual Tauri smoke test**

Run: `pnpm run tauri:dev`

Expected:
- simple docx still enters the editor and writes back
- rich docx with styles/lists/formulas imports successfully into the workbench
- rich docx renders bold/italic/underline/protected placeholders in the review panes
- rich docx finalize writes back safely
- rich docx plain-text editor entry fails with the explicit reason from the session payload

- [ ] **Step 4: Commit the final verification pass**

```bash
git add .
git commit -m "验证 docx 文章语义支持"
```
