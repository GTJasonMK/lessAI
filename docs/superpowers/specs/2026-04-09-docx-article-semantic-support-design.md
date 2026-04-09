# Docx Article Semantic Support Design

## Goal

Expand `.docx` support from “simple plain paragraphs” to an article-oriented subset that improves in-app readability and rewrite coverage, without turning LessAI into a Word compatibility layer.

## Product Boundary

The feature serves one purpose only: help users read article-like documents inside the project and run AI rewriting on natural-language text. Word-specific collaboration or layout features are not goals.

## Content Classes

- Editable text: article text that may be rewritten. Includes headings, paragraphs, list item text, hyperlink display text, captions, and other article-bearing text.
- Locked text: visible text that must never be rewritten. Includes formulas, hyperlink URLs, and structural list markers.
- Structural placeholders: non-text article objects shown for readability but never rewritten. Includes images, tables, fields, page breaks, and section breaks.
- Ignored content: not shown and not processed. Includes headers, footers, and page numbers.
- Hard-reject content: unsupported Word features that must fail import. Includes tracked changes, comments, footnotes, endnotes, SmartArt, charts, embedded objects, and other complex Office-only structures.

Article-bearing text inside containers such as text boxes should be extracted as readable text when it is still plain article content. Their Word-specific floating/layout semantics are not preserved.

## Supported Reading Behavior

- Keep common inline formatting semantics for article text: bold, italic, underline.
- Keep hyperlink structure, but only rewrite the visible link text; the target URL remains locked.
- Support ordered and unordered lists as article text, while preserving numbering/bullets as locked structure.
- Show formulas as visible locked content.
- Show non-text article objects with placeholders. Prefer caption-aware forms such as `[图片：图1 xxx]` and `[表格：表2 xxx]`; fall back to generic placeholders like `[图片]` and `[表格]`.
- Represent fields and similar generated structures with placeholders such as `[目录]`, `[交叉引用]`, `[文献域]`, and `[自动编号]`.
- Show `[分页符]` and `[分节符]` placeholders to preserve reading structure.

## Architecture

Do not add a new docx-specific rewrite pipeline. Reuse the existing repository flow:

`docx adapter -> TextRegion(skip_rewrite) -> segment_regions -> rewrite -> writeback`

The docx adapter should internally parse XML into an article-semantic model with block-level nodes and inline-level nodes, then flatten that model into the existing `TextRegion` abstraction:

- editable article text becomes normal regions
- formulas and other locked content become visible `skip_rewrite` regions or locked placeholders
- non-text article objects become visible placeholder regions

Do not add a second placeholder-locking mechanism for docx. Reuse the existing “locked region -> placeholder mask -> rewrite -> restore locked content” design already used for markdown and tex, and keep docx-specific logic limited to XML parsing, semantic flattening, and anchor-based writeback.

Writeback must use XML anchors collected during import. Only editable text nodes may change. Locked text, formulas, URLs, list structure, and placeholder-backed objects must round-trip unchanged. If anchor mapping becomes ambiguous, writeback fails explicitly.

## Non-Goals

- No support for review/collaboration features
- No support for full Word layout fidelity
- No silent downgrade when structure cannot be mapped safely

## Testing

- Parse article-style docx containing inline styles, lists, formulas, hyperlinks, captions, and placeholders.
- Verify formulas stay visible but never enter rewriteable text.
- Verify hyperlink text may change while URL remains stable.
- Verify image/table/field/break placeholders preserve readable structure.
- Reject tracked changes, comments, footnotes, endnotes, and complex Office objects at import time.
- Reject writeback whenever XML anchors no longer map safely.
