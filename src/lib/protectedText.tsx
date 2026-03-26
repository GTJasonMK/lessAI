import type { ReactNode } from "react";

export type ClientDocumentFormat = "plain" | "markdown" | "tex";

export function guessClientDocumentFormat(documentPath: string): ClientDocumentFormat {
  const lowered = (documentPath ?? "").trim().toLowerCase();
  const dot = lowered.lastIndexOf(".");
  const ext = dot >= 0 ? lowered.slice(dot + 1) : "";

  if (ext === "md" || ext === "markdown") return "markdown";
  if (ext === "tex" || ext === "latex") return "tex";
  return "plain";
}

type ProtectedSegment =
  | { kind: "text"; text: string }
  | { kind: "protected"; text: string; label: string; protectKind: string };

const TEX_MATH_ENV_NAMES = new Set([
  "equation",
  "equation*",
  "align",
  "align*",
  "alignat",
  "alignat*",
  "flalign",
  "flalign*",
  "gather",
  "gather*",
  "multline",
  "multline*",
  "eqnarray",
  "eqnarray*",
  "math",
  "displaymath",
  "split",
  "cases",
  "matrix",
  "pmatrix",
  "bmatrix",
  "vmatrix",
  "Vmatrix"
]);

// 与后端 TexAdapter::raw_env_names 保持大体一致：
// 这些环境内部往往包含大量非自然语言内容（代码/表格/绘图/算法等），应视为保护区。
const TEX_RAW_ENV_NAMES = new Set([
  "verbatim",
  "verbatim*",
  "Verbatim",
  "Verbatim*",
  "minted",
  "minted*",
  "lstlisting",
  "lstlisting*",
  "comment",
  "filecontents",
  "filecontents*",
  "tabular",
  "tabular*",
  "longtable",
  "tabu",
  "array",
  "tikzpicture",
  "tikzpicture*",
  "pgfpicture",
  "pgfpicture*",
  "forest",
  "forest*",
  "algorithm",
  "algorithm*",
  "algorithmic",
  "algorithmic*",
  "thebibliography",
  "thebibliography*",
  "bibliography",
  "references"
]);

function isEscaped(text: string, index: number): boolean {
  if (index <= 0) return false;
  let backslashes = 0;
  let i = index - 1;
  while (i >= 0 && text[i] === "\\") {
    backslashes += 1;
    i -= 1;
  }
  return backslashes % 2 === 1;
}

function countRun(text: string, start: number, ch: string): number {
  let len = 0;
  while (start + len < text.length && text[start + len] === ch) len += 1;
  return len;
}

function findBacktickClosing(text: string, from: number, runLen: number): number | null {
  if (runLen <= 0) return null;
  let index = from;
  while (index < text.length) {
    const ch = text[index];
    if (ch === "\n" || ch === "\r") return null;
    if (ch !== "`") {
      index += 1;
      continue;
    }
    const candidate = countRun(text, index, "`");
    if (candidate === runLen) return index + runLen;
    index += Math.max(candidate, 1);
  }
  return null;
}

function findMatchingGroup(
  text: string,
  start: number,
  openCh: string,
  closeCh: string
): number | null {
  if (start < 0 || start >= text.length) return null;
  if (text[start] !== openCh) return null;

  let depth = 1;
  let index = start + 1;
  while (index < text.length) {
    const ch = text[index];
    if (ch === "\n" || ch === "\r") return null;

    if (ch === "\\") {
      index += 2;
      continue;
    }
    if (ch === openCh) {
      depth += 1;
      index += 1;
      continue;
    }
    if (ch === closeCh) {
      depth -= 1;
      index += 1;
      if (depth === 0) return index;
      continue;
    }
    index += 1;
  }
  return null;
}

function findMarkdownLinkEnd(text: string, start: number): number | null {
  let index = start;
  if (text[index] === "!") {
    if (text[index + 1] !== "[") return null;
    index += 1;
  }
  if (text[index] !== "[") return null;

  const close = findMatchingGroup(text, index, "[", "]") ?? null;
  if (close == null) return null;

  let pos = close;
  while (pos < text.length && (text[pos] === " " || text[pos] === "\t")) pos += 1;
  if (pos >= text.length) return null;

  if (text[pos] === "(") {
    return findMatchingGroup(text, pos, "(", ")");
  }
  if (text[pos] === "[") {
    return findMatchingGroup(text, pos, "[", "]");
  }
  return null;
}

function findHtmlCommentEnd(text: string, start: number): number | null {
  if (!text.startsWith("<!--", start)) return null;
  let index = start + "<!--".length;
  while (index < text.length) {
    const ch = text[index];
    if (ch === "\n" || ch === "\r") return null;
    if (text.startsWith("-->", index)) return index + "-->".length;
    index += 1;
  }
  return null;
}

function findMarkdownAutolinkEnd(text: string, start: number): number | null {
  if (text[start] !== "<") return null;
  let index = start + 1;
  while (index < text.length) {
    const ch = text[index];
    if (ch === "\n" || ch === "\r") return null;
    if (ch === ">") {
      const inner = text.slice(start + 1, index).trim().toLowerCase();
      if (
        inner.startsWith("http://") ||
        inner.startsWith("https://") ||
        inner.startsWith("mailto:")
      ) {
        return index + 1;
      }
      return null;
    }
    index += 1;
  }
  return null;
}

function findInlineHtmlTagEnd(text: string, start: number): number | null {
  if (text[start] !== "<") return null;
  const next = text[start + 1];
  if (!next) return null;
  const looksLikeTag = next === "/" || /[A-Za-z]/.test(next);
  if (!looksLikeTag) return null;

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
  while (end > start && /[.,;:!?，。；：！？]/.test(text[end - 1])) {
    end -= 1;
  }
  return end > start ? end : null;
}

function findMarkdownInlineMathEnd(text: string, start: number): number | null {
  if (text[start] !== "$") return null;
  if (isEscaped(text, start)) return null;

  const delimiterLen = text[start + 1] === "$" ? 2 : 1;
  let index = start + delimiterLen;
  while (index < text.length) {
    const ch = text[index];
    if (ch === "\n" || ch === "\r") return null;
    if (ch !== "$") {
      index += 1;
      continue;
    }
    if (isEscaped(text, index)) {
      index += 1;
      continue;
    }
    if (delimiterLen === 2) {
      if (text[index + 1] === "$" && !isEscaped(text, index)) {
        if (index > start + delimiterLen) return index + 2;
        return null;
      }
      index += 1;
      continue;
    }
    if (index > start + delimiterLen) return index + 1;
    return null;
  }
  return null;
}

function findMarkdownCitationOrFootnoteEnd(text: string, start: number): number | null {
  if (text[start] !== "[") return null;
  const marker = text[start + 1];
  if (marker !== "^" && marker !== "@") return null;

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

function splitMarkdownInlineProtected(text: string): ProtectedSegment[] {
  const segments: ProtectedSegment[] = [];
  let cursor = 0;

  let atLineStart = true;
  let lineStartIndex = 0;
  let index = 0;
  while (index < text.length) {
    const ch = text[index];

    let end: number | null = null;
    let start = index;
    let label = "";
    let protectKind = "";

    if (atLineStart && ch === "$" && text[index + 1] === "$") {
      const lineEnd = findLineEndIndex(text, lineStartIndex);
      const line = text.slice(lineStartIndex, lineEnd);
      if (line.trim() === "$$") {
        const openLineEnd = lineEnd + lineEndingLength(text, lineEnd);
        const close = findMarkdownMathBlockEnd(text, openLineEnd);
        if (close != null && close > openLineEnd) {
          start = lineStartIndex;
          end = close;
          label = "数学块（$$ ... $$）";
          protectKind = "math-block";
        }
      }
    } else if (ch === "`") {
      const runLen = countRun(text, index, "`");
      const close = findBacktickClosing(text, index + runLen, runLen);
      if (close != null && close > index + runLen) {
        end = close;
        label = "行内代码（`...`）";
        protectKind = "code";
      }
    } else if (ch === "[" && (text[index + 1] === "^" || text[index + 1] === "@")) {
      const close = findMarkdownCitationOrFootnoteEnd(text, index);
      if (close != null) {
        end = close;
        label = text[index + 1] === "^" ? "脚注引用（[^...]）" : "引用标记（[@...]）";
        protectKind = text[index + 1] === "^" ? "footnote" : "citation";
      }
    } else if (ch === "!" || ch === "[") {
      const close = findMarkdownLinkEnd(text, index);
      if (close != null) {
        end = close;
        label = "链接/图片语法";
        protectKind = "link";
      }
    } else if (ch === "<") {
      const commentEnd = findHtmlCommentEnd(text, index);
      if (commentEnd != null) {
        end = commentEnd;
        label = "HTML 注释";
        protectKind = "html-comment";
      } else {
        const autolinkEnd = findMarkdownAutolinkEnd(text, index);
        if (autolinkEnd != null) {
          end = autolinkEnd;
          label = "自动链接";
          protectKind = "autolink";
        } else {
          const tagEnd = findInlineHtmlTagEnd(text, index);
          if (tagEnd != null) {
            end = tagEnd;
            label = "HTML 标签";
            protectKind = "html-tag";
          }
        }
      }
    } else if (ch === "h") {
      if (text.startsWith("http://", index) || text.startsWith("https://", index)) {
        const close = findBareUrlEnd(text, index);
        if (close != null) {
          end = close;
          label = "URL";
          protectKind = "url";
        }
      }
    } else if (ch === "w") {
      if (text.startsWith("www.", index)) {
        const close = findBareUrlEnd(text, index);
        if (close != null) {
          end = close;
          label = "URL";
          protectKind = "url";
        }
      }
    } else if (ch === "$") {
      const close = findMarkdownInlineMathEnd(text, index);
      if (close != null) {
        end = close;
        label = "数学公式（$...$）";
        protectKind = "math";
      }
    }

    if (end != null && end > start) {
      if (start > cursor) {
        segments.push({ kind: "text", text: text.slice(cursor, start) });
      }
      segments.push({
        kind: "protected",
        text: text.slice(start, end),
        label,
        protectKind
      });
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
      index += 1;
      continue;
    }

    if (atLineStart) {
      if (ch === " " || ch === "\t") {
        index += 1;
        continue;
      }
      atLineStart = false;
    }

    index += 1;
  }

  if (cursor < text.length) {
    segments.push({ kind: "text", text: text.slice(cursor) });
  }

  return segments.length > 0 ? segments : [{ kind: "text", text }];
}

function lineEndingLength(text: string, lineEndIndex: number): number {
  if (lineEndIndex >= text.length) return 0;
  const ch = text[lineEndIndex];
  if (ch === "\r") {
    return text[lineEndIndex + 1] === "\n" ? 2 : 1;
  }
  if (ch === "\n") return 1;
  return 0;
}

function findMarkdownMathBlockEnd(text: string, from: number): number | null {
  let index = from;
  while (index < text.length) {
    const lineEnd = findLineEndIndex(text, index);
    const line = text.slice(index, lineEnd);
    if (line.trim() === "$$") {
      return lineEnd + lineEndingLength(text, lineEnd);
    }
    index = lineEnd + lineEndingLength(text, lineEnd);
  }
  return null;
}

function findLineEndIndex(text: string, start: number): number {
  let index = start;
  while (index < text.length) {
    const ch = text[index];
    if (ch === "\n" || ch === "\r") return index;
    index += 1;
  }
  return text.length;
}

function findSubstringSameLine(text: string, from: number, needle: string): number | null {
  const lineEnd = findLineEndIndex(text, from);
  const offset = text.slice(from, lineEnd).indexOf(needle);
  if (offset < 0) return null;
  return from + offset + needle.length;
}

function findSubstringAnywhere(text: string, from: number, needle: string): number | null {
  const offset = text.indexOf(needle, from);
  if (offset < 0) return null;
  return offset + needle.length;
}

function findTexDoubleDollarBlockEnd(text: string, start: number): number | null {
  if (!text.startsWith("$$", start)) return null;
  if (isEscaped(text, start)) return null;
  let index = start + 2;
  while (index + 1 < text.length) {
    if (text[index] === "$" && text[index + 1] === "$" && !isEscaped(text, index)) {
      if (index > start + 2) return index + 2;
      return null;
    }
    index += 1;
  }
  return null;
}

function findTexInlineMathEnd(text: string, start: number): number | null {
  if (text[start] !== "$") return null;
  if (isEscaped(text, start)) return null;

  const delimiterLen = text[start + 1] === "$" ? 2 : 1;
  let index = start + delimiterLen;
  while (index < text.length) {
    const ch = text[index];
    if (ch === "\n" || ch === "\r") return null;
    if (ch !== "$") {
      index += 1;
      continue;
    }
    if (isEscaped(text, index)) {
      index += 1;
      continue;
    }
    if (delimiterLen === 2) {
      if (text[index + 1] === "$" && !isEscaped(text, index)) {
        if (index > start + delimiterLen) return index + 2;
        return null;
      }
      index += 1;
      continue;
    }
    if (index > start + delimiterLen) return index + 1;
    return null;
  }
  return null;
}

function splitTexInlineProtected(text: string): ProtectedSegment[] {
  const segments: ProtectedSegment[] = [];
  let cursor = 0;

  let index = 0;
  while (index < text.length) {
    const ch = text[index];

    let end: number | null = null;
    let label = "";
    let protectKind = "";

    if (ch === "%" && !isEscaped(text, index)) {
      const lineEnd = findLineEndIndex(text, index);
      if (lineEnd > index) {
        end = lineEnd;
        label = "TeX 注释（% ...）";
        protectKind = "tex-comment";
      }
    } else if (ch === "\\" && text[index + 1] === "\\") {
      // `\\` 换行命令（可选 `*` 与可选 `[len]`）
      if (!isEscaped(text, index)) {
        let pos = index + 2;
        if (text[pos] === "*") pos += 1;

        while (pos < text.length && (text[pos] === " " || text[pos] === "\t")) pos += 1;
        if (text[pos] === "[") {
          const close = findMatchingGroup(text, pos, "[", "]");
          if (close != null) {
            pos = close;
          }
        }

        end = pos;
        label = "TeX 换行（\\\\ ...）";
        protectKind = "tex-linebreak";
      }
    } else if (ch === "\\" && text.startsWith("\\begin{", index) && !isEscaped(text, index)) {
      const nameStart = index + "\\begin{".length;
      const nameEnd = text.indexOf("}", nameStart);
      if (nameEnd > nameStart) {
        const envName = text.slice(nameStart, nameEnd);
        if (TEX_MATH_ENV_NAMES.has(envName) || TEX_RAW_ENV_NAMES.has(envName)) {
          const pattern = `\\end{${envName}}`;
          const close = findSubstringAnywhere(text, nameEnd + 1, pattern);
          if (close != null && close > nameEnd + 1) {
            end = close;
            if (TEX_MATH_ENV_NAMES.has(envName)) {
              label = `TeX 数学环境（${envName}）`;
              protectKind = "tex-math-block";
            } else {
              label = `TeX 原样环境（${envName}）`;
              protectKind = "tex-raw-env";
            }
          }
        }
      }
    } else if (ch === "$" && text[index + 1] === "$") {
      const close = findTexDoubleDollarBlockEnd(text, index);
      if (close != null) {
        end = close;
        label = "TeX 数学块（$$ ... $$）";
        protectKind = "tex-math-block";
      }
    } else if (ch === "$") {
      const close = findTexInlineMathEnd(text, index);
      if (close != null) {
        end = close;
        label = "TeX 数学公式（$...$）";
        protectKind = "tex-math";
      }
    } else if (ch === "\\" && text[index + 1] === "(" && !isEscaped(text, index)) {
      const close = findSubstringSameLine(text, index + 2, "\\)");
      if (close != null && close > index + 2) {
        end = close;
        label = "TeX 数学公式（\\( ... \\)）";
        protectKind = "tex-math";
      }
    } else if (ch === "\\" && text[index + 1] === "[" && !isEscaped(text, index)) {
      const close = findSubstringAnywhere(text, index + 2, "\\]");
      if (close != null && close > index + 2) {
        end = close;
        label = "TeX 数学公式（\\[ ... \\]）";
        protectKind = "tex-math-block";
      }
    }

    if (end != null && end > index) {
      if (index > cursor) {
        segments.push({ kind: "text", text: text.slice(cursor, index) });
      }
      segments.push({
        kind: "protected",
        text: text.slice(index, end),
        label,
        protectKind
      });
      cursor = end;
      index = end;
      continue;
    }

    index += 1;
  }

  if (cursor < text.length) {
    segments.push({ kind: "text", text: text.slice(cursor) });
  }

  return segments.length > 0 ? segments : [{ kind: "text", text }];
}

export function renderInlineProtectedText(
  text: string,
  format: ClientDocumentFormat,
  keyPrefix = "protected"
): ReactNode {
  if (format === "markdown") {
    const likelyHasProtected =
      text.includes("`") ||
      text.includes("$") ||
      text.includes("[") ||
      text.includes("!") ||
      text.includes("<") ||
      text.includes("http://") ||
      text.includes("https://") ||
      text.includes("www.");
    if (!likelyHasProtected) return text;
  }

  if (format === "tex") {
    const likelyHasProtected =
      text.includes("$") ||
      text.includes("%") ||
      text.includes("\\(") ||
      text.includes("\\[") ||
      text.includes("\\\\") ||
      text.includes("\\begin{");
    if (!likelyHasProtected) return text;
  }

  const segments =
    format === "markdown"
      ? splitMarkdownInlineProtected(text)
      : format === "tex"
        ? splitTexInlineProtected(text)
        : null;
  if (!segments) return text;
  if (segments.length === 1 && segments[0].kind === "text") return text;

  return segments.map((segment, index) => {
    if (segment.kind === "text") return segment.text;
    return (
      <span
        key={`${keyPrefix}-${index}-${segment.text.length}`}
        className="inline-protected"
        data-protect-kind={segment.protectKind}
        title={`保护区：${segment.label}，AI 不会修改`}
      >
        {segment.text}
      </span>
    );
  });
}
