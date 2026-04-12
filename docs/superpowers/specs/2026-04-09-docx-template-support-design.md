# Docx Template Support Design

## Goal

Make LessAI able to open `testdoc/04-3 作品报告（大数据应用赛，2025版）模板.docx`, process article text, and safely write back to the original `.docx` without exposing unsupported structures as editable content.

## Non-Goals

- Full Microsoft Word compatibility
- Editing images, text boxes, tables, TOC fields, comments, or revision marks
- Recovering arbitrary floating-layout semantics into editable prose

## Guiding Rules

- Only article-related plain text is editable.
- Non-text or layout-driven structures are shown as locked placeholders.
- Anything editable in UI must remain safely write-backable.
- Unsupported high-risk structures must still fail explicitly.

## Target Coverage For This Template

### Editable

- Body paragraphs and headings
- List item text
- Common inline styles already supported today
- Hyperlink display text
- Visible tabs and inline line breaks

### Locked Placeholders

- Images and drawings
- Text boxes
- Table of contents blocks and field-based generated content
- Tables
- Formulas
- Page breaks and section breaks

### Ignored Metadata

- `bookmarkStart` / `bookmarkEnd`
- `proofErr`
- field boundary helpers that do not carry visible article text

## Import Design

Extend docx import from “strict simple paragraph only” to “article-semantic blocks plus locked objects”.

- Block-level parser accepts `w:p`, `w:tbl`, `w:sectPr`, `w:sdt`, and drawing/textbox containers.
- `w:tbl`, `w:sdt`, drawings, and text boxes become locked placeholder regions with explicit `protect_kind`.
- Paragraph parsing tolerates non-content markers such as bookmarks and proofing tags instead of rejecting the whole paragraph.
- Text inside text boxes is not promoted into editable article content; the whole textbox becomes one locked placeholder.
- TOC content controls are imported as one locked placeholder, not expanded into editable lines.

## Writeback Design

- Only editable body text regions participate in text replacement.
- Locked placeholders keep original XML and must survive round-trip unchanged.
- Existing snapshot check and atomic write remain mandatory.
- If a structure cannot be mapped to either editable text, locked placeholder, or ignorable metadata, import or writeback must fail explicitly.

## Required Parser Changes

1. Add block classifiers for textbox, TOC/content controls, and drawing-backed objects.
2. Teach paragraph/run parsers to ignore `bookmark*`, `proofErr`, and safe field markers.
3. Add placeholder kinds for `image`, `textbox`, and `toc`.
4. Preserve writeback templates so locked objects never become editable regions.

## Testing

- Add a fixture-based regression using `04-3 作品报告（大数据应用赛，2025版）模板.docx`.
- Verify import succeeds and produces editable正文 + locked placeholders.
- Verify textbox, TOC, image, and table content never enter editable rewrite chunks.
- Verify applying正文 edits still writes back successfully.
- Verify unsupported embedded Office objects still fail explicitly.
