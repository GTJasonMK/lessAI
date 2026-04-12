import {
  findLineEndIndex,
  findMatchingGroup,
  findSubstringAnywhere,
  findSubstringSameLine,
  isEscaped,
  type ProtectedSegment
} from "./protectedTextShared";

type CommandName = { name: string | null; end: number };
type ProtectedSpan = { end: number; label: string; protectKind: string };
type SplitCommand = { end: number; parts: ProtectedSegment[] };

const TEX_MATH_ENV_NAMES = new Set([
  "equation", "equation*", "align", "align*", "alignat", "alignat*", "flalign", "flalign*",
  "gather", "gather*", "multline", "multline*", "eqnarray", "eqnarray*", "math",
  "displaymath", "split", "cases", "matrix", "pmatrix", "bmatrix", "vmatrix", "Vmatrix"
]);

const TEX_RAW_ENV_NAMES = new Set([
  "verbatim", "verbatim*", "Verbatim", "Verbatim*", "minted", "minted*", "lstlisting",
  "lstlisting*", "comment", "filecontents", "filecontents*", "tabular", "tabular*",
  "longtable", "tabu", "array", "tikzpicture", "tikzpicture*", "pgfpicture", "pgfpicture*",
  "forest", "forest*", "algorithm", "algorithm*", "algorithmic", "algorithmic*",
  "thebibliography", "thebibliography*", "bibliography", "references"
]);

const EDITABLE_TEXT_COMMANDS = new Set([
  "footnote", "emph", "textbf", "textit", "underline", "textrm", "textsf", "textsc"
]);

export function splitTexInlineProtected(text: string): ProtectedSegment[] {
  const segments: ProtectedSegment[] = [];
  let cursor = 0;
  let index = 0;

  while (index < text.length) {
    const splitCommand = findEditableTextCommand(text, index);
    if (splitCommand) {
      if (index > cursor) segments.push({ kind: "text", text: text.slice(cursor, index) });
      segments.push(...splitCommand.parts);
      cursor = splitCommand.end;
      index = splitCommand.end;
      continue;
    }

    const protectedSpan = findProtectedSpan(text, index);
    if (protectedSpan) {
      if (index > cursor) segments.push({ kind: "text", text: text.slice(cursor, index) });
      segments.push({
        kind: "protected",
        text: text.slice(index, protectedSpan.end),
        label: protectedSpan.label,
        protectKind: protectedSpan.protectKind
      });
      cursor = protectedSpan.end;
      index = protectedSpan.end;
      continue;
    }

    index += 1;
  }

  if (cursor < text.length) segments.push({ kind: "text", text: text.slice(cursor) });
  return segments.length > 0 ? segments : [{ kind: "text", text }];
}

function findProtectedSpan(text: string, index: number): ProtectedSpan | null {
  return (
    findCommentSpan(text, index) ??
    findLinebreakSpan(text, index) ??
    findEnvironmentSpan(text, index) ??
    findMathSpan(text, index) ??
    findDelimitedCommandSpan(text, index) ??
    findGenericCommandSpan(text, index)
  );
}

function findCommentSpan(text: string, index: number): ProtectedSpan | null {
  if (text[index] !== "%" || isEscaped(text, index)) return null;
  const end = findLineEndIndex(text, index);
  if (end <= index) return null;
  return { end, label: "TeX 注释（% ...）", protectKind: "tex-comment" };
}

function findLinebreakSpan(text: string, index: number): ProtectedSpan | null {
  if (!text.startsWith("\\\\", index) || isEscaped(text, index)) return null;
  let end = index + 2;
  if (text[end] === "*") end += 1;
  end = consumeWhitespace(text, end);
  if (text[end] === "[") end = parseBracketGroup(text, end) ?? text.length;
  return { end, label: "TeX 换行（\\\\ ...）", protectKind: "tex-linebreak" };
}

function findEnvironmentSpan(text: string, index: number): ProtectedSpan | null {
  if (!text.startsWith("\\begin{", index) || isEscaped(text, index)) return null;
  const nameStart = index + "\\begin{".length;
  const nameEnd = text.indexOf("}", nameStart);
  if (nameEnd <= nameStart) return null;
  const envName = text.slice(nameStart, nameEnd);
  if (!TEX_MATH_ENV_NAMES.has(envName) && !TEX_RAW_ENV_NAMES.has(envName)) return null;
  const end = findSubstringAnywhere(text, nameEnd + 1, `\\end{${envName}}`) ?? text.length;
  const protectKind = TEX_MATH_ENV_NAMES.has(envName) ? "tex-math-block" : "tex-raw-env";
  const label = TEX_MATH_ENV_NAMES.has(envName) ? `TeX 数学环境（${envName}）` : `TeX 原样环境（${envName}）`;
  return { end, label, protectKind };
}

function findMathSpan(text: string, index: number): ProtectedSpan | null {
  const doubleDollar = findDoubleDollarSpan(text, index);
  if (doubleDollar) return doubleDollar;
  const singleDollar = findSingleDollarSpan(text, index);
  if (singleDollar) return singleDollar;
  const parenMath = findDelimitedMathSpan(text, index, "\\(", "\\)", "tex-math");
  if (parenMath) return parenMath;
  return findDelimitedMathSpan(text, index, "\\[", "\\]", "tex-math-block");
}

function findDoubleDollarSpan(text: string, index: number): ProtectedSpan | null {
  if (!text.startsWith("$$", index) || isEscaped(text, index)) return null;
  let cursor = index + 2;
  while (cursor + 1 < text.length) {
    if (text[cursor] === "$" && text[cursor + 1] === "$" && !isEscaped(text, cursor)) {
      return { end: cursor + 2, label: "TeX 数学块（$$ ... $$）", protectKind: "tex-math-block" };
    }
    cursor += 1;
  }
  return null;
}

function findSingleDollarSpan(text: string, index: number): ProtectedSpan | null {
  if (text[index] !== "$" || text[index + 1] === "$" || isEscaped(text, index)) return null;
  let cursor = index + 1;
  while (cursor < text.length) {
    if (text[cursor] === "\n" || text[cursor] === "\r") return null;
    if (text[cursor] === "$" && !isEscaped(text, cursor)) {
      if (cursor <= index + 1) return null;
      return { end: cursor + 1, label: "TeX 数学公式（$...$）", protectKind: "tex-math" };
    }
    cursor += 1;
  }
  return null;
}

function findDelimitedMathSpan(
  text: string,
  index: number,
  open: string,
  close: string,
  protectKind: string
): ProtectedSpan | null {
  if (!text.startsWith(open, index) || isEscaped(text, index)) return null;
  const end =
    close === "\\)" ? findSubstringSameLine(text, index + open.length, close) : findSubstringAnywhere(text, index + open.length, close);
  if (end == null || end <= index + open.length) return null;
  const label = close === "\\)" ? "TeX 数学公式（\\( ... \\)）" : "TeX 数学公式（\\[ ... \\]）";
  return { end, label, protectKind };
}

function findDelimitedCommandSpan(text: string, index: number): ProtectedSpan | null {
  const verbEnd = findInlineVerbEnd(text, index);
  if (verbEnd != null) return { end: verbEnd, label: "TeX 行内原样命令", protectKind: "tex-verbatim" };
  const lstEnd = findInlineDelimitedCommandEnd(text, index, "\\lstinline");
  if (lstEnd != null) return { end: lstEnd, label: "TeX 行内代码命令", protectKind: "tex-inline-code" };
  const pathEnd = findInlineDelimitedCommandEnd(text, index, "\\path");
  if (pathEnd != null) return { end: pathEnd, label: "TeX 路径命令", protectKind: "tex-path" };
  return null;
}

function findEditableTextCommand(text: string, index: number): SplitCommand | null {
  const command = parseCommandName(text, index);
  if (!command?.name || !EDITABLE_TEXT_COMMANDS.has(command.name)) return null;
  const groupStart = skipOptionalGroups(text, command.end);
  if (text[groupStart] !== "{") return null;
  const groupEnd = parseBraceGroup(text, groupStart);
  if (groupEnd == null || groupEnd <= groupStart + 1) return null;
  const contentStart = groupStart + 1;
  const contentEnd = groupEnd - 1;
  return {
    end: groupEnd,
    parts: [
      protectedPart(text.slice(index, contentStart), "TeX 文本命令语法", "tex-command"),
      ...splitTexInlineProtected(text.slice(contentStart, contentEnd)),
      protectedPart(text.slice(contentEnd, groupEnd), "TeX 文本命令闭合", "tex-command")
    ]
  };
}

function findGenericCommandSpan(text: string, index: number): ProtectedSpan | null {
  const end = findCommandSpanEnd(text, index);
  if (end == null || end <= index) return null;
  return { end, label: "TeX 命令", protectKind: "tex-command" };
}

function findInlineVerbEnd(text: string, index: number): number | null {
  if (!text.startsWith("\\verb", index)) return null;
  let cursor = index + "\\verb".length;
  if (text[cursor] === "*") cursor += 1;
  if (cursor >= text.length || /\s/.test(text[cursor])) return null;
  const delimiter = text[cursor];
  cursor += 1;
  while (cursor < text.length) {
    if (text[cursor] === delimiter) return cursor + 1;
    cursor += 1;
  }
  return text.length;
}

function findInlineDelimitedCommandEnd(text: string, index: number, command: string): number | null {
  if (!text.startsWith(command, index)) return null;
  let cursor = index + command.length;
  if (text[cursor] === "*") cursor += 1;
  cursor = skipOptionalGroups(text, cursor);
  if (cursor >= text.length || /\s/.test(text[cursor]) || /[{}]/.test(text[cursor])) return null;
  const delimiter = text[cursor];
  cursor += 1;
  while (cursor < text.length) {
    if (text[cursor] === delimiter) return cursor + 1;
    cursor += 1;
  }
  return text.length;
}

function findCommandSpanEnd(text: string, index: number): number | null {
  const command = parseCommandName(text, index);
  if (!command) return null;
  if (command.name == null) return findControlSymbolEnd(text, index, command.end);
  let cursor = command.end;
  while (true) {
    const next = skipOptionalGroups(text, cursor);
    if (next >= text.length) return next;
    if (text[next] !== "{") return next;
    const groupEnd = parseBraceGroup(text, next);
    if (groupEnd == null) return text.length;
    cursor = groupEnd;
  }
}

function findControlSymbolEnd(text: string, index: number, end: number): number {
  if (!text.startsWith("\\\\", index)) return end;
  let cursor = end;
  if (text[cursor] === "*") cursor += 1;
  cursor = consumeWhitespace(text, cursor);
  if (text[cursor] !== "[") return cursor;
  return parseBracketGroup(text, cursor) ?? text.length;
}

function parseCommandName(text: string, index: number): CommandName | null {
  if (text[index] !== "\\") return null;
  const start = index + 1;
  if (start >= text.length) return null;
  if (!/[A-Za-z]/.test(text[start])) return { name: null, end: start + 1 };
  let end = start + 1;
  while (end < text.length && /[A-Za-z]/.test(text[end])) end += 1;
  if (text[end] === "*") end += 1;
  return { name: text.slice(start, end).replace(/\*$/, ""), end };
}

function skipOptionalGroups(text: string, index: number): number {
  let cursor = consumeWhitespace(text, index);
  while (text[cursor] === "[") {
    const next = parseBracketGroup(text, cursor);
    if (next == null) return text.length;
    cursor = consumeWhitespace(text, next);
  }
  return cursor;
}

function consumeWhitespace(text: string, index: number): number {
  let cursor = index;
  while (cursor < text.length && /[\t\n\r ]/.test(text[cursor])) cursor += 1;
  return cursor;
}

function parseBracketGroup(text: string, start: number): number | null {
  return findMatchingGroup(text, start, "[", "]");
}

function parseBraceGroup(text: string, start: number): number | null {
  if (start >= text.length || text[start] !== "{") return null;
  let depth = 1;
  let cursor = start + 1;

  while (cursor < text.length) {
    const ch = text[cursor];
    if (ch === "\\") {
      cursor += 2;
      continue;
    }
    if (ch === "{") depth += 1;
    if (ch === "}") depth -= 1;
    cursor += 1;
    if (depth === 0) return cursor;
  }

  return text.length;
}

function protectedPart(text: string, label: string, protectKind: string): ProtectedSegment {
  return { kind: "protected", text, label, protectKind };
}
