import { normalizeNewlines } from "./webBridgeTextCore";
import type { RewriteUnit, SegmentationPreset, WritebackSlot } from "./types";

const MIN_REWRITE_UNIT_CHARS = 4;

export function splitTextByParagraphSeparator(text: string) {
  const chunks: string[] = [];
  let start = 0;
  while (start < text.length) {
    const matched = findNextParagraphSeparator(text, start);
    if (!matched) {
      break;
    }
    chunks.push(text.slice(start, matched.end));
    start = matched.end;
  }
  if (start < text.length || chunks.length === 0) {
    chunks.push(text.slice(start));
  }
  return chunks;
}

export function readChar(text: string, index: number, limit = text.length) {
  if (index < 0 || index >= limit) {
    return null;
  }
  const codePoint = text.codePointAt(index);
  if (codePoint == null) {
    return null;
  }
  const char = String.fromCodePoint(codePoint);
  const len = char.length;
  if (index + len > limit) {
    return null;
  }
  return { char, len };
}

export function consumeLineBreak(text: string, index: number) {
  const char = text[index];
  if (char === "\n") {
    return index + 1;
  }
  if (char === "\r") {
    return text[index + 1] === "\n" ? index + 2 : index + 1;
  }
  return null;
}

export function findNextParagraphSeparator(text: string, from: number) {
  let index = from;
  while (index < text.length) {
    const firstEnd = consumeLineBreak(text, index);
    if (firstEnd == null) {
      index += 1;
      continue;
    }

    let probe = firstEnd;
    while (probe < text.length && (text[probe] === " " || text[probe] === "\t")) {
      probe += 1;
    }

    const secondEnd = consumeLineBreak(text, probe);
    if (secondEnd == null) {
      index = firstEnd;
      continue;
    }

    let end = secondEnd;
    while (end < text.length) {
      let next = end;
      while (next < text.length && (text[next] === " " || text[next] === "\t")) {
        next += 1;
      }
      const nextEnd = consumeLineBreak(text, next);
      if (nextEnd == null) {
        break;
      }
      end = nextEnd;
    }
    return { start: index, end };
  }
  return null;
}

export function trailingParagraphSeparatorRange(text: string) {
  const range = findNextParagraphSeparator(text, 0);
  if (!range) return null;
  return range.end === text.length ? range : null;
}

export function splitTextAndTrailingSeparator(text: string) {
  const paragraphRange = trailingParagraphSeparatorRange(text);
  if (paragraphRange) {
    return {
      body: text.slice(0, paragraphRange.start),
      separatorAfter: text.slice(paragraphRange.start, paragraphRange.end)
    };
  }

  let splitAt = text.length;
  while (splitAt > 0 && /\s/.test(text[splitAt - 1] ?? "")) {
    splitAt -= 1;
  }
  return {
    body: text.slice(0, splitAt),
    separatorAfter: text.slice(splitAt)
  };
}

const CLAUSE_BOUNDARIES = new Set(["。", "！", "？", "；", "!", "?", ";", ".", "，", ","]);
const SENTENCE_BOUNDARIES = new Set(["。", "！", "？", "；", "!", "?", ";", "."]);
const CLOSING_PUNCTUATION = new Set([
  "\"",
  "'",
  "”",
  "’",
  "）",
  ")",
  "】",
  "]",
  "}",
  "」",
  "』",
  "》",
  "〉"
]);

export function previousChar(text: string, index: number) {
  let cursor = 0;
  let previous: string | null = null;
  while (cursor < index) {
    const info = readChar(text, cursor, index);
    if (!info) {
      break;
    }
    previous = info.char;
    cursor += info.len;
  }
  return previous;
}

export function nextChar(text: string, index: number, limit: number) {
  return readChar(text, Math.min(index, limit), limit)?.char ?? null;
}

export function nextNonWhitespaceChar(text: string, index: number, limit: number) {
  let cursor = Math.min(index, limit);
  while (cursor < limit) {
    const info = readChar(text, cursor, limit);
    if (!info) {
      break;
    }
    if (!/\s/.test(info.char)) {
      return info.char;
    }
    cursor += info.len;
  }
  return null;
}

export function asciiWordLenBefore(text: string, index: number) {
  let count = 0;
  const chars = Array.from(text.slice(0, index)).reverse();
  for (const char of chars) {
    if (!/[A-Za-z]/.test(char)) {
      break;
    }
    count += 1;
  }
  return count;
}

export function consumeClosingPunctuation(text: string, index: number, limit: number) {
  let nextIndex = index;
  while (nextIndex < limit) {
    const info = readChar(text, nextIndex, limit);
    if (!info || !CLOSING_PUNCTUATION.has(info.char)) {
      break;
    }
    nextIndex += info.len;
  }
  return nextIndex;
}

export function consumeInlineWhitespace(text: string, index: number, limit: number) {
  let nextIndex = index;
  while (nextIndex < limit) {
    const info = readChar(text, nextIndex, limit);
    if (!info || !/\s/.test(info.char)) {
      break;
    }
    nextIndex += info.len;
  }
  return nextIndex;
}

export function isSplitBoundary(
  text: string,
  index: number,
  char: string,
  limit: number,
  preset: Exclude<SegmentationPreset, "paragraph">
) {
  const allowed = preset === "clause" ? CLAUSE_BOUNDARIES : SENTENCE_BOUNDARIES;
  if (!allowed.has(char)) {
    return false;
  }

  if (char === "." || char === ",") {
    const prev = previousChar(text, index);
    const next = nextNonWhitespaceChar(text, index + char.length, limit);

    if (prev && /\d/.test(prev) && next && /\d/.test(next)) {
      return false;
    }
    if (char === ".") {
      if (nextChar(text, index + char.length, limit) === "\\") {
        return false;
      }
      if (prev && /[A-Za-z]/.test(prev) && next && /[A-Za-z]/.test(next)) {
        return false;
      }
      if (
        prev &&
        /[A-Za-z]/.test(prev) &&
        next &&
        /[A-Z]/.test(next) &&
        asciiWordLenBefore(text, index) <= 2
      ) {
        return false;
      }
    }
  }

  return true;
}

export function splitParagraphChunkByBoundary(
  text: string,
  preset: Exclude<SegmentationPreset, "paragraph">
) {
  const range = trailingParagraphSeparatorRange(text);
  const contentLimit = range ? range.start : text.length;
  const chunks: string[] = [];
  let start = 0;
  let index = 0;

  while (index < contentLimit) {
    const info = readChar(text, index, contentLimit);
    if (!info) {
      break;
    }
    if (!isSplitBoundary(text, index, info.char, contentLimit, preset)) {
      index += info.len;
      continue;
    }

    let end = index + info.len;
    end = consumeClosingPunctuation(text, end, contentLimit);
    end = consumeInlineWhitespace(text, end, contentLimit);
    chunks.push(text.slice(start, end));
    start = end;
    index = end;
  }

  if (start < text.length || chunks.length === 0) {
    chunks.push(text.slice(start));
  }
  return chunks;
}

export function containsParagraphSeparator(text: string) {
  return findNextParagraphSeparator(text, 0) != null;
}

export function buildBoundaryAwareSlots(sourceText: string) {
  const normalized = normalizeNewlines(sourceText);
  const slots: WritebackSlot[] = [];

  const paragraphChunks = splitTextByParagraphSeparator(normalized);
  for (let paragraphIndex = 0; paragraphIndex < paragraphChunks.length; paragraphIndex += 1) {
    const paragraphChunk = paragraphChunks[paragraphIndex];
    const pieces = splitParagraphChunkByBoundary(paragraphChunk, "clause");

    for (let pieceIndex = 0; pieceIndex < pieces.length; pieceIndex += 1) {
      const piece = pieces[pieceIndex];
      const { body, separatorAfter } = splitTextAndTrailingSeparator(piece);
      if (!body && separatorAfter) {
        const last = slots[slots.length - 1];
        if (last) {
          last.separatorAfter += separatorAfter;
        }
        continue;
      }

      const textEmpty = body.length === 0;
      const whitespaceOnly =
        !textEmpty && Array.from(body).every((char) => /\s/.test(char));
      const editable = !whitespaceOnly && !textEmpty;
      const role = !editable && containsParagraphSeparator(separatorAfter)
        ? "paragraphBreak"
        : editable
          ? "editableText"
          : "lockedText";
      const anchor = `txt:p${paragraphIndex}:r0:s${pieceIndex}`;

      slots.push({
        id: anchor,
        order: slots.length,
        text: body,
        editable,
        role,
        presentation: null,
        anchor,
        separatorAfter
      });
    }
  }

  return slots;
}

export function displayTextFromSlots(slots: WritebackSlot[]) {
  return slots.map((slot) => `${slot.text}${slot.separatorAfter}`).join("");
}

export function hasInlineLineBreakBoundary(slot: WritebackSlot) {
  return slot.anchor != null && slot.separatorAfter.includes("\n");
}

export function endsSemanticGroup(
  slots: WritebackSlot[],
  preset: SegmentationPreset
) {
  const chars = Array.from(displayTextFromSlots(slots));
  while (chars.length > 0 && /\s/.test(chars[chars.length - 1] ?? "")) {
    chars.pop();
  }
  while (
    chars.length > 0 &&
    CLOSING_PUNCTUATION.has(chars[chars.length - 1] ?? "")
  ) {
    chars.pop();
  }
  const last = chars[chars.length - 1];
  if (!last) {
    return false;
  }
  if (preset === "clause") {
    return CLAUSE_BOUNDARIES.has(last);
  }
  if (preset === "sentence") {
    return SENTENCE_BOUNDARIES.has(last);
  }
  return false;
}

export function shouldCloseUnit(current: WritebackSlot[], preset: SegmentationPreset) {
  const last = current[current.length - 1];
  if (!last) {
    return false;
  }
  if (last.role === "paragraphBreak" || containsParagraphSeparator(last.separatorAfter)) {
    return true;
  }
  if (preset === "paragraph") {
    return false;
  }
  if (hasInlineLineBreakBoundary(last)) {
    return true;
  }
  return endsSemanticGroup(current, preset);
}

export function isStandaloneSeparatorUnit(current: WritebackSlot[]) {
  return (
    current.length === 1 &&
    current[0].role === "paragraphBreak" &&
    current[0].text === ""
  );
}

export function isBlankLockedUnit(current: WritebackSlot[]) {
  return (
    current.every((slot) => !slot.editable) &&
    displayTextFromSlots(current).trim() === ""
  );
}

export function shouldSkipUnit(current: WritebackSlot[]) {
  return isStandaloneSeparatorUnit(current) || isBlankLockedUnit(current);
}

export function unitGroupCharCount(group: WritebackSlot[]) {
  return Array.from(displayTextFromSlots(group).trim()).length;
}

export function mergeShortUnitGroups(groups: WritebackSlot[][]) {
  if (groups.length <= 1) {
    return;
  }
  let index = 0;
  while (index < groups.length) {
    if (unitGroupCharCount(groups[index]) >= MIN_REWRITE_UNIT_CHARS) {
      index += 1;
      continue;
    }
    if (index + 1 < groups.length) {
      const current = groups.splice(index, 1)[0];
      groups[index] = current.concat(groups[index]);
      continue;
    }
    index += 1;
  }
}

export function buildRewriteUnits(slots: WritebackSlot[], preset: SegmentationPreset) {
  const groups: WritebackSlot[][] = [];
  let current: WritebackSlot[] = [];

  for (const slot of slots) {
    current.push(slot);
    if (!shouldCloseUnit(current, preset)) {
      continue;
    }
    if (shouldSkipUnit(current)) {
      current = [];
      continue;
    }
    groups.push(current);
    current = [];
  }

  if (current.length > 0 && !shouldSkipUnit(current)) {
    groups.push(current);
  }

  if (preset !== "paragraph") {
    mergeShortUnitGroups(groups);
  }

  return groups.map((group, order): RewriteUnit => ({
    id: `unit-${order}`,
    order,
    slotIds: group.map((slot) => slot.id),
    displayText: displayTextFromSlots(group),
    segmentationPreset: preset,
    status: group.some((slot) => slot.editable) ? "idle" : "done",
    errorMessage: null
  }));
}

export function buildStructure(sourceText: string, preset: SegmentationPreset) {
  const writebackSlots = buildBoundaryAwareSlots(sourceText);
  const rewriteUnits = buildRewriteUnits(writebackSlots, preset);
  return { writebackSlots, rewriteUnits };
}
