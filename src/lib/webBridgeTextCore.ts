import type { DocumentSnapshot } from "./types";

export function simpleHash(input: string) {
  let hash = 0;
  for (let index = 0; index < input.length; index += 1) {
    hash = (hash << 5) - hash + input.charCodeAt(index);
    hash |= 0;
  }
  return Math.abs(hash).toString(16);
}

export function normalizeNewlines(text: string) {
  return text.replace(/\r\n/g, "\n").replace(/\r/g, "\n");
}

export function normalizeText(input: string) {
  const normalized = normalizeNewlines(input);
  const lines: string[] = [];
  let blankStreak = 0;

  for (const rawLine of normalized.split("\n")) {
    const trimmed = rawLine.trim();
    if (!trimmed) {
      blankStreak += 1;
      if (blankStreak <= 1) {
        lines.push("");
      }
      continue;
    }
    blankStreak = 0;
    lines.push(trimmed);
  }

  return lines.join("\n").trim();
}

export function snapshotFromText(text: string): DocumentSnapshot {
  return {
    sha256: simpleHash(normalizeNewlines(text))
  };
}
export function detectDominantLineEnding(text: string) {
  const crlf = (text.match(/\r\n/g) ?? []).length;
  const cr = (text.match(/\r(?!\n)/g) ?? []).length;
  const lf = (text.match(/(?<!\r)\n/g) ?? []).length;
  if (crlf === 0 && cr === 0 && lf === 0) {
    return "\n";
  }
  if (crlf >= lf && crlf >= cr) {
    return "\r\n";
  }
  if (lf >= cr) {
    return "\n";
  }
  return "\r";
}

export function hasTrailingSpacesPerLine(text: string) {
  return /[ \t]+(?:\r\n|\r|\n|$)/.test(text);
}

export function stripTrailingSpacesPerLine(text: string) {
  return text
    .split(/\r\n|\r|\n/)
    .map((line) => line.replace(/[ \t]+$/g, ""))
    .join("\n");
}

export function normalizeTextAgainstSourceLayout(source: string, candidate: string) {
  const ending = detectDominantLineEnding(source);
  let normalized = normalizeNewlines(candidate);
  if (!hasTrailingSpacesPerLine(source)) {
    normalized = stripTrailingSpacesPerLine(normalized);
  }
  if (ending === "\n") {
    return normalized;
  }
  return normalized.replace(/\n/g, ending);
}
