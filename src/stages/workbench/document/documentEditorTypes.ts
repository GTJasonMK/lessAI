import type { EditorChunkOverrides } from "../../../lib/editorChunks";
import type { DocumentSession, EditorChunkEdit } from "../../../lib/types";

export interface DocumentEditorSelectionSnapshotBase {
  text: string;
  startOffset: number;
  endOffset: number;
}

export interface PlainTextSelectionSnapshot extends DocumentEditorSelectionSnapshotBase {
  kind: "text";
}

export interface ChunkSelectionSnapshot extends DocumentEditorSelectionSnapshotBase {
  kind: "chunk";
  chunkIndex: number;
}

export type DocumentEditorSelectionSnapshot =
  | PlainTextSelectionSnapshot
  | ChunkSelectionSnapshot;

export type DocumentEditorApplyResult =
  | { ok: true }
  | { ok: false; error: string };

export type DocumentEditorPreviewResult =
  | { ok: true; value: string; chunkEdits?: EditorChunkEdit[] }
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
  collectChunkEdits: () => EditorChunkEdit[] | null;
}

export interface DocumentEditorProps {
  session: DocumentSession;
  value: string;
  chunkOverrides: EditorChunkOverrides;
  dirty: boolean;
  busy: boolean;
  onChange: (value: string) => void;
  onChangeChunkText: (index: number, value: string) => void;
  onSave: () => void;
  onSelectionChange?: (hasSelection: boolean) => void;
}
