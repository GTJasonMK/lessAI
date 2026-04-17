import type { EditorSlotOverrides } from "../../../lib/editorSlots";
import type { DocumentSession, EditorSlotEdit } from "../../../lib/types";

export interface DocumentEditorSelectionSnapshotBase {
  text: string;
  startOffset: number;
  endOffset: number;
}

export interface PlainTextSelectionSnapshot extends DocumentEditorSelectionSnapshotBase {
  kind: "text";
}

export interface SlotSelectionSnapshot extends DocumentEditorSelectionSnapshotBase {
  kind: "slot";
  slotId: string;
}

export type DocumentEditorSelectionSnapshot =
  | PlainTextSelectionSnapshot
  | SlotSelectionSnapshot;

export type DocumentEditorApplyResult =
  | { ok: true }
  | { ok: false; error: string };

export type DocumentEditorPreviewResult =
  | { ok: true; value: string; slotEdits?: EditorSlotEdit[] }
  | { ok: false; error: string };

export interface DocumentEditorHandle {
  captureSelection: () => DocumentEditorSelectionSnapshot | null;
  previewSelectionReplacement: (
    snapshot: DocumentEditorSelectionSnapshot,
    replacementText: string
  ) => DocumentEditorPreviewResult;
  applySelectionReplacement: (
    snapshot: DocumentEditorSelectionSnapshot,
    replacementText: string
  ) => DocumentEditorApplyResult;
  collectSlotEdits: () => EditorSlotEdit[] | null;
}

export interface DocumentEditorProps {
  session: DocumentSession;
  value: string;
  slotOverrides: EditorSlotOverrides;
  dirty: boolean;
  busy: boolean;
  onChange: (value: string) => void;
  onChangeSlotText: (slotId: string, value: string) => void;
  onSave: () => void;
  onSelectionChange?: (hasSelection: boolean) => void;
}
