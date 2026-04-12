import {
  countRun,
  findBacktickClosing,
  findLineEndIndex,
  findMatchingGroup,
  isEscaped,
  lineEndingLength,
  type ProtectedSegment
} from "./protectedTextShared";

function findMarkdownLinkEnd(text: string, start: number): number | null {
  let index = start;
  if (text[index] === "!") {
    if (text[index + 1] !== "[") return null;
    index += 1;
  }
  if (text[index] !== "[") return null;

  const close = findMatchingGroup(text, index, "[", "]");
  if (close == null) return null;

  let pos = close;
  while (pos < text.length && (text[pos] === " " || text[pos] === "\t")) pos += 1;
  if (pos >= text.length) return null;
  if (text[pos] === "(") return findMatchingGroup(text, pos, "(", ")");
  if (text[pos] === "[") return findMatchingGroup(text, pos, "[", "]");
  return null;
}

function findHtmlCommentEnd(text: string, start: number): number | null {
  if (!text.startsWith("<!--", start)) return null;
  const offset = text.indexOf("-->", start + "<!--".length);
  if (offset < 0) return null;
  return text.slice(start, offset).includes("\n") ? null : offset + 3;
}

function findMarkdownAutolinkEnd(text: string, start: number): number | null {
  if (text[start] !== "<") return null;
  let index = start + 1;

  while (index < text.length) {
    const ch = text[index];
    if (ch === "\n" || ch === "\r") return null;
    if (ch === ">") {
      const inner = text.slice(start + 1, index).trim().toLowerCase();
      const allowed =
        inner.startsWith("http://") ||
        inner.startsWith("https://") ||
        inner.startsWith("mailto:");
      return allowed ? index + 1 : null;
    }
    index += 1;
  }

  return null;
}

function findInlineHtmlTagEnd(text: string, start: number): number | null {
  const next = text[start + 1];
  if (text[start] !== "<" || !next) return null;
  if (next !== "/" && !/[A-Za-z]/.test(next)) return null;

  let index = start + 2;
  while (index < text.length) {
    const ch = text[index];
    if (ch === "\n" || ch === "\r") return null;
    if (ch === ">") return index + 1;
    index += 1;
  }

  return null;
}

function findBareUrlEnd(text: string, start: number): number | null {
  let end = start;
  while (end < text.length) {
    const ch = text[end];
    if (/\s/.test(ch) || ch === "<" || ch === ">" || ch === '"' || ch === "'" || ch === "]") {
      break;
    }
    end += 1;
  }
  while (end > start && /[.,;:!?，。；：！？]/.test(text[end - 1])) end -= 1;
  return end > start ? end : null;
}

function findMarkdownInlineMathEnd(text: string, start: number): number | null {
  if (text[start] !== "$" || isEscaped(text, start)) return null;

  const delimiterLen = text[start + 1] === "$" ? 2 : 1;
  let index = start + delimiterLen;
  while (index < text.length) {
    if (text[index] !== "$") {
      index += 1;
      continue;
    }
    if (isEscaped(text, index)) {
      index += 1;
      continue;
    }
    if (delimiterLen === 2) return text[index + 1] === "$" ? index + 2 : null;
    return index > start + delimiterLen ? index + 1 : null;
  }

  return null;
}

function findMarkdownCitationOrFootnoteEnd(text: string, start: number): number | null {
  if (text[start] !== "[" || !["^", "@"].includes(text[start + 1] ?? "")) return null;
  let index = start + 2;

  while (index < text.length) {
    const ch = text[index];
    if (ch === "\n" || ch === "\r") return null;
    if (ch === "\\") {
      index += 2;
      continue;
    }
    if (ch === "]") return index + 1;
    index += 1;
  }

  return null;
}

function findMarkdownMathBlockEnd(text: string, from: number): number | null {
  let index = from;
  while (index < text.length) {
    const lineEnd = findLineEndIndex(text, index);
    if (text.slice(index, lineEnd).trim() === "$$") {
      return lineEnd + lineEndingLength(text, lineEnd);
    }
    index = lineEnd + lineEndingLength(text, lineEnd);
  }
  return null;
}

export function splitMarkdownInlineProtected(text: string): ProtectedSegment[] {
  const segments: ProtectedSegment[] = [];
  let cursor = 0;
  let index = 0;
  let atLineStart = true;
  let lineStartIndex = 0;

  while (index < text.length) {
    const ch = text[index];
    let start = index;
    let end: number | null = null;
    let label = "";
    let protectKind = "";

    if (atLineStart && ch === "$" && text[index + 1] === "$") {
      const lineEnd = findLineEndIndex(text, lineStartIndex);
      const isFence = text.slice(lineStartIndex, lineEnd).trim() === "$$";
      const openLineEnd = lineEnd + lineEndingLength(text, lineEnd);
      if (isFence) end = findMarkdownMathBlockEnd(text, openLineEnd);
      if (end != null) {
        start = lineStartIndex;
        label = "数学块（$$ ... $$）";
        protectKind = "math-block";
      }
    } else if (ch === "`") {
      const close = findBacktickClosing(text, index + countRun(text, index, "`"), countRun(text, index, "`"));
      if (close != null) {
        end = close;
        label = "行内代码（`...`）";
        protectKind = "code";
      }
    } else if (ch === "[" && (text[index + 1] === "^" || text[index + 1] === "@")) {
      end = findMarkdownCitationOrFootnoteEnd(text, index);
      if (end != null) {
        label = text[index + 1] === "^" ? "脚注引用（[^...]）" : "引用标记（[@...]）";
        protectKind = text[index + 1] === "^" ? "footnote" : "citation";
      }
    } else if (ch === "!" || ch === "[") {
      end = findMarkdownLinkEnd(text, index);
      if (end != null) {
        label = "链接/图片语法";
        protectKind = "link";
      }
    } else if (ch === "<") {
      end = findHtmlCommentEnd(text, index) ?? findMarkdownAutolinkEnd(text, index) ?? findInlineHtmlTagEnd(text, index);
      if (end != null) {
        label = text.startsWith("<!--", index)
          ? "HTML 注释"
          : text[index + 1] === "!" || text[index + 1] === "/" || /[A-Za-z]/.test(text[index + 1] ?? "")
            ? "HTML 标签"
            : "自动链接";
        protectKind = text.startsWith("<!--", index)
          ? "html-comment"
          : text[index] === "<" && /^(https?:\/\/|mailto:)/i.test(text.slice(index + 1, end - 1))
            ? "autolink"
            : "html-tag";
      }
    } else if ((ch === "h" && /^https?:\/\//.test(text.slice(index))) || (ch === "w" && text.startsWith("www.", index))) {
      end = findBareUrlEnd(text, index);
      if (end != null) {
        label = "URL";
        protectKind = "url";
      }
    } else if (ch === "$") {
      end = findMarkdownInlineMathEnd(text, index);
      if (end != null) {
        label = "数学公式（$...$）";
        protectKind = "math";
      }
    }

    if (end != null && end > start) {
      if (start > cursor) segments.push({ kind: "text", text: text.slice(cursor, start) });
      segments.push({ kind: "protected", text: text.slice(start, end), label, protectKind });
      cursor = end;
      index = end;
      if (protectKind === "math-block") {
        atLineStart = true;
        lineStartIndex = index;
      }
      continue;
    }

    if (ch === "\n" || ch === "\r") {
      atLineStart = true;
      lineStartIndex = index + 1;
    } else if (atLineStart && ch !== " " && ch !== "\t") {
      atLineStart = false;
    }
    index += 1;
  }

  if (cursor < text.length) segments.push({ kind: "text", text: text.slice(cursor) });
  return segments.length > 0 ? segments : [{ kind: "text", text }];
}
