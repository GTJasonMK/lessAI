# Simple Docx Writeback Design

## Goal

Support safe writeback for simple `.docx` files without corrupting the original package. Unsupported structures must fail at import time with a clear error.

## Supported Scope

- Only `word/document.xml` body paragraphs (`w:p`) and trailing section properties (`w:sectPr`)
- Paragraph text made from plain text runs only
- Optional paragraph styles such as `Heading1`, `Title`, `Subtitle`
- Empty paragraphs

## Explicitly Unsupported

- Tables (`w:tbl`)
- Numbered or bulleted lists (`w:numPr`)
- Text boxes, drawings, hyperlinks, fields, bookmarks, content controls
- Track changes (`w:ins`, `w:del`), comments, footnote/endnote references
- Inline formatting or complex run structure that cannot be rewritten losslessly
- Line-wrapped pseudo-paragraph docx that previously depended on soft-wrap coalescing

## Design

Import parses `word/document.xml` into a strict `SimpleDocxDocument` model. The parser extracts paragraph text, heading flags, and validates that the body contains only supported structures. If validation fails, import returns an error and no session is created.

Writeback re-reads the original `.docx`, validates it again, and checks that the extracted source text still matches the session source text. This prevents overwriting a file that changed outside the app. The writer then rewrites only the paragraph text nodes in `word/document.xml`, keeps the rest of the zip entries untouched, and writes the updated package back to disk atomically through the existing save flow.

## UI Behavior

- Simple docx may enter editor mode and may finalize back to the original file
- Unsupported docx fails during open with a direct error
- PDF remains read-only import/export only

## Testing

- Accept simple heading/body docx and write back updated text
- Reject tables, numbered lists, tracked changes, and complex runs
- Reject writeback if the source file no longer matches the session text
