import { forwardRef, memo, useCallback, useEffect, useImperativeHandle, useRef } from "react";
import type { ClipboardEvent } from "react";

import { normalizeNewlines } from "../../../lib/helpers";
import type {
  DocumentEditorHandle,
  DocumentEditorProps,
  DocumentEditorSelectionSnapshot,
  DocumentEditorPreviewResult,
} from "./documentEditorTypes";
import { buildSelectionSnapshotBase } from "./editorSelectionShared";

function buildSelectionSnapshot(
  node: HTMLDivElement,
  range: Range
): DocumentEditorSelectionSnapshot | null {
  const base = buildSelectionSnapshotBase(node, range);
  if (!base) return null;

  return {
    kind: "text",
    ...base
  };
}

function previewReplacementValue(
  node: HTMLDivElement,
  snapshot: DocumentEditorSelectionSnapshot,
  replacementText: string
): DocumentEditorPreviewResult {
  if (snapshot.kind !== "text") {
    return { ok: false, error: "当前选区类型与编辑器不匹配，请重新选中后再试。" };
  }

  const replacement = normalizeNewlines(replacementText);
  if (replacement.trim().length === 0) {
    return { ok: false, error: "模型返回内容为空，已取消替换。" };
  }

  const currentValue = normalizeNewlines(node.innerText);
  const currentSelected = currentValue.slice(snapshot.startOffset, snapshot.endOffset);
  if (currentSelected !== snapshot.text) {
    return { ok: false, error: "选区已变化或文本已被修改，请重新选中后再试。" };
  }

  return {
    ok: true,
    value: `${currentValue.slice(0, snapshot.startOffset)}${replacement}${currentValue.slice(
      snapshot.endOffset
    )}`
  };
}

export const PlainTextDocumentEditor = memo(
  forwardRef<DocumentEditorHandle, DocumentEditorProps>(function PlainTextDocumentEditor(
    { value, dirty, busy, onChange, onSave, onSelectionChange }: DocumentEditorProps,
    ref
  ) {
    const editorFieldRef = useRef<HTMLDivElement | null>(null);
    const hasSelectionRef = useRef(false);

    useEffect(() => {
      const handleKeyDown = (event: KeyboardEvent) => {
        const key = event.key.toLowerCase();
        if (!(event.ctrlKey || event.metaKey) || key !== "s") return;
        event.preventDefault();
        if (!dirty || busy) return;
        onSave();
      };

      window.addEventListener("keydown", handleKeyDown);
      return () => window.removeEventListener("keydown", handleKeyDown);
    }, [busy, dirty, onSave]);

    useEffect(() => {
      const node = editorFieldRef.current;
      if (!node) return;

      const domText = normalizeNewlines(node.innerText);
      if (domText === value) return;
      if (document.activeElement === node && dirty) return;
      node.innerText = value;
    }, [dirty, value]);

    useEffect(() => {
      editorFieldRef.current?.focus();
    }, []);

    useEffect(() => {
      if (!onSelectionChange) return;

      const handleSelectionChange = () => {
        const node = editorFieldRef.current;
        if (!node) return;

        const selection = window.getSelection();
        const range = selection?.rangeCount ? selection.getRangeAt(0) : null;
        const withinEditor =
          !!range &&
          node.contains(range.startContainer) &&
          node.contains(range.endContainer) &&
          !range.collapsed;
        if (withinEditor === hasSelectionRef.current) return;
        hasSelectionRef.current = withinEditor;
        onSelectionChange(withinEditor);
      };

      document.addEventListener("selectionchange", handleSelectionChange);
      return () => document.removeEventListener("selectionchange", handleSelectionChange);
    }, [onSelectionChange]);

    useImperativeHandle(
      ref,
      (): DocumentEditorHandle => ({
        captureSelection: () => {
          const node = editorFieldRef.current;
          const selection = window.getSelection();
          if (!node || !selection?.rangeCount) return null;
          return buildSelectionSnapshot(node, selection.getRangeAt(0));
        },

        previewSelectionReplacement: (snapshot, replacementText) => {
          const node = editorFieldRef.current;
          if (!node) return { ok: false, error: "编辑器尚未就绪。" };
          return previewReplacementValue(node, snapshot, replacementText);
        },

        applySelectionReplacement: (snapshot, replacementText) => {
          const node = editorFieldRef.current;
          if (!node) return { ok: false, error: "编辑器尚未就绪。" };

          const preview = previewReplacementValue(node, snapshot, replacementText);
          if (!preview.ok) return preview;

          node.innerText = preview.value;
          node.focus();
          onChange(preview.value);
          return { ok: true };
        },

        collectSlotEdits: () => null
      }),
      [onChange]
    );

    const handleEditorInput = useCallback(() => {
      const node = editorFieldRef.current;
      if (!node) return;
      onChange(normalizeNewlines(node.innerText));
    }, [onChange]);

    const handleEditorPaste = useCallback((event: ClipboardEvent<HTMLDivElement>) => {
      event.preventDefault();
      const text = event.clipboardData.getData("text/plain");
      if (!text) return;

      if (document.execCommand("insertText", false, text)) return;
      const selection = window.getSelection();
      if (!selection?.rangeCount) return;
      selection.deleteFromDocument();
      selection.getRangeAt(0).insertNode(document.createTextNode(text));
      selection.collapseToEnd();
    }, []);

    return (
      <div
        ref={editorFieldRef}
        className={`document-flow workbench-editor-editable ${
          value.trim().length === 0 ? "is-empty" : ""
        }`}
        contentEditable={!busy}
        role="textbox"
        aria-multiline="true"
        aria-label="编辑终稿"
        tabIndex={0}
        spellCheck={false}
        data-placeholder="在此编辑终稿…"
        onInput={handleEditorInput}
        onPaste={handleEditorPaste}
        suppressContentEditableWarning
      />
    );
  })
);
