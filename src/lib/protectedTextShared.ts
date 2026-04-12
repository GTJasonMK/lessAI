export type ProtectedSegment =
  | { kind: "text"; text: string }
  | { kind: "protected"; text: string; label: string; protectKind: string };

export function isEscaped(text: string, index: number): boolean {
  if (index <= 0) return false;
  let backslashes = 0;
  let cursor = index - 1;

  while (cursor >= 0 && text[cursor] === "\\") {
    backslashes += 1;
    cursor -= 1;
  }

  return backslashes % 2 === 1;
}

export function countRun(text: string, start: number, ch: string): number {
  let len = 0;
  while (start + len < text.length && text[start + len] === ch) {
    len += 1;
  }
  return len;
}

export function findBacktickClosing(
  text: string,
  from: number,
  runLen: number
): number | null {
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

export function findMatchingGroup(
  text: string,
  start: number,
  openCh: string,
  closeCh: string
): number | null {
  if (start < 0 || start >= text.length || text[start] !== openCh) return null;

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

export function findLineEndIndex(text: string, start: number): number {
  let index = start;
  while (index < text.length) {
    const ch = text[index];
    if (ch === "\n" || ch === "\r") return index;
    index += 1;
  }
  return text.length;
}

export function lineEndingLength(text: string, lineEndIndex: number): number {
  if (lineEndIndex >= text.length) return 0;
  const ch = text[lineEndIndex];
  if (ch === "\r") return text[lineEndIndex + 1] === "\n" ? 2 : 1;
  return ch === "\n" ? 1 : 0;
}

export function findSubstringSameLine(
  text: string,
  from: number,
  needle: string
): number | null {
  const lineEnd = findLineEndIndex(text, from);
  const offset = text.slice(from, lineEnd).indexOf(needle);
  return offset >= 0 ? from + offset + needle.length : null;
}

export function findSubstringAnywhere(
  text: string,
  from: number,
  needle: string
): number | null {
  const offset = text.indexOf(needle, from);
  return offset >= 0 ? offset + needle.length : null;
}
