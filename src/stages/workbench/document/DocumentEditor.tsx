import { memo, useCallback, useEffect, useRef } from "react";
import type { ClipboardEvent } from "react";
import { normalizeNewlines } from "../../../lib/helpers";

interface DocumentEditorProps {
  value: string;
  dirty: boolean;
  busy: boolean;
  onChange: (value: string) => void;
  onSave: () => void;
}

export const DocumentEditor = memo(function DocumentEditor({
  value,
  dirty,
  busy,
  onChange,
  onSave
}: DocumentEditorProps) {
  const editorFieldRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      const key = event.key.toLowerCase();
      const saveCombo = (event.ctrlKey || event.metaKey) && key === "s";
      if (!saveCombo) return;

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
    const node = editorFieldRef.current;
    if (!node) return;

    requestAnimationFrame(() => {
      node.focus();
    });
  }, []);

  const handleEditorInput = useCallback(() => {
    const node = editorFieldRef.current;
    if (!node) return;
    onChange(normalizeNewlines(node.innerText));
  }, [onChange]);

  const handleEditorPaste = useCallback((event: ClipboardEvent<HTMLDivElement>) => {
    event.preventDefault();
    const text = event.clipboardData.getData("text/plain");
    if (!text) return;

    const ok = document.execCommand("insertText", false, text);
    if (ok) return;

    const selection = window.getSelection();
    if (!selection || selection.rangeCount === 0) return;
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
      contentEditable
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
});

