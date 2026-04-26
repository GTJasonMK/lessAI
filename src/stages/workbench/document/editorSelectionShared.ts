import { normalizeNewlines } from "../../../lib/helpers";
import type { DocumentEditorSelectionSnapshotBase } from "./documentEditorTypes";

function selectionPointOffset(root: Node, container: Node, offset: number) {
  const range = document.createRange();
  range.selectNodeContents(root);
  range.setEnd(container, offset);
  return normalizeNewlines(range.toString()).length;
}

function normalizeSelectionComparableText(text: string) {
  return normalizeNewlines(text).replace(/[\u00a0\u202f]/g, " ");
}

function findNearestMatchOffset(source: string, needle: string, anchor: number) {
  let from = 0;
  let bestStart = -1;
  let bestDistance = Number.POSITIVE_INFINITY;
  while (from <= source.length) {
    const hit = source.indexOf(needle, from);
    if (hit < 0) break;
    const distance = Math.abs(hit - anchor);
    if (distance < bestDistance) {
      bestDistance = distance;
      bestStart = hit;
      if (distance === 0) break;
    }
    from = hit + 1;
  }
  return { bestStart, bestDistance };
}

export function resolveSnapshotRangeInText(
  currentText: string,
  snapshot: DocumentEditorSelectionSnapshotBase
) {
  const comparableCurrent = normalizeSelectionComparableText(currentText);
  const comparableSnapshotText = normalizeSelectionComparableText(snapshot.text);
  if (!comparableSnapshotText) {
    return null;
  }

  const directSelected = comparableCurrent.slice(snapshot.startOffset, snapshot.endOffset);
  if (directSelected === comparableSnapshotText) {
    return {
      startOffset: snapshot.startOffset,
      endOffset: snapshot.endOffset
    };
  }

  const { bestStart, bestDistance } = findNearestMatchOffset(
    comparableCurrent,
    comparableSnapshotText,
    snapshot.startOffset
  );
  if (bestStart < 0) return null;

  // Keep a conservative guard to avoid replacing a far-away duplicate when content has drifted.
  if (bestDistance > 256) {
    return null;
  }

  return {
    startOffset: bestStart,
    endOffset: bestStart + comparableSnapshotText.length
  };
}

export function buildSelectionSnapshotBase(
  root: Node,
  range: Range
): DocumentEditorSelectionSnapshotBase | null {
  if (range.collapsed) return null;
  if (!root.contains(range.startContainer) || !root.contains(range.endContainer)) {
    return null;
  }

  const text = normalizeNewlines(range.toString());
  if (text.trim().length === 0) return null;

  return {
    text: normalizeSelectionComparableText(text),
    startOffset: selectionPointOffset(root, range.startContainer, range.startOffset),
    endOffset: selectionPointOffset(root, range.endContainer, range.endOffset)
  };
}
