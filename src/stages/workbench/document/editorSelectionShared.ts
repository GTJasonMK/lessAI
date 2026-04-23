import { normalizeNewlines } from "../../../lib/helpers";
import type { DocumentEditorSelectionSnapshotBase } from "./documentEditorTypes";

function selectionPointOffset(root: Node, container: Node, offset: number) {
  const range = document.createRange();
  range.selectNodeContents(root);
  range.setEnd(container, offset);
  return normalizeNewlines(range.toString()).length;
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
    text,
    startOffset: selectionPointOffset(root, range.startContainer, range.startOffset),
    endOffset: selectionPointOffset(root, range.endContainer, range.endOffset)
  };
}
