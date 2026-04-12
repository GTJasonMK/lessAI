import { Fragment, forwardRef, memo, useCallback, useEffect, useImperativeHandle, useRef } from "react";
import type { ClipboardEvent } from "react";

import {
  applyEditorChunkOverride,
  buildEditorChunkEdits,
  buildEditorTextFromChunks,
  resolveEditorChunkText,
} from "../../../lib/editorChunks";
import { normalizeNewlines } from "../../../lib/helpers";
import type { ChunkTask } from "../../../lib/types";
import type {
  ChunkSelectionSnapshot,
  DocumentEditorHandle,
  DocumentEditorProps,
  DocumentEditorSelectionSnapshot,
  DocumentEditorPreviewResult,
} from "./documentEditorTypes";

function selectionPointOffset(node: HTMLElement, container: Node, offset: number) {
  const range = document.createRange();
  range.selectNodeContents(node);
  range.setEnd(container, offset);
  return normalizeNewlines(range.toString()).length;
}

function buildChunkSelectionSnapshot(
  node: HTMLElement,
  chunkIndex: number,
  range: Range
): ChunkSelectionSnapshot | null {
  if (range.collapsed) return null;
  if (!node.contains(range.startContainer) || !node.contains(range.endContainer)) {
    return null;
  }

  const text = normalizeNewlines(range.toString());
  if (text.trim().length === 0) return null;

  return {
    kind: "chunk",
    chunkIndex,
    text,
    startOffset: selectionPointOffset(node, range.startContainer, range.startOffset),
    endOffset: selectionPointOffset(node, range.endContainer, range.endOffset)
  };
}

function replaceChunkSelectionText(
  currentText: string,
  snapshot: ChunkSelectionSnapshot,
  replacementText: string
) {
  const replacement = normalizeNewlines(replacementText);
  if (replacement.trim().length === 0) {
    return { ok: false, error: "模型返回内容为空，已取消替换。" } as const;
  }

  const selected = currentText.slice(snapshot.startOffset, snapshot.endOffset);
  if (selected !== snapshot.text) {
    return { ok: false, error: "选区已变化或文本已被修改，请重新选中后再试。" } as const;
  }

  return {
    ok: true,
    text: `${currentText.slice(0, snapshot.startOffset)}${replacement}${currentText.slice(
      snapshot.endOffset
    )}`
  } as const;
}

function chunkPresentationClass(chunk: ChunkTask) {
  const presentation = chunk.presentation;
  return [
    "docx-editor-chunk",
    chunk.skipRewrite ? "is-locked" : "is-editable",
    presentation?.bold ? "is-bold" : "",
    presentation?.italic ? "is-italic" : "",
    presentation?.underline ? "is-underline" : "",
    presentation?.href ? "is-link" : ""
  ]
    .filter(Boolean)
    .join(" ");
}

const EditableChunkSpan = memo(function EditableChunkSpan({
  chunk,
  text,
  busy,
  registerNode,
  onChange,
}: {
  chunk: ChunkTask;
  text: string;
  busy: boolean;
  registerNode: (index: number, node: HTMLSpanElement | null) => void;
  onChange: (index: number, value: string) => void;
}) {
  const nodeRef = useRef<HTMLSpanElement | null>(null);

  useEffect(() => {
    registerNode(chunk.index, nodeRef.current);
    return () => registerNode(chunk.index, null);
  }, [chunk.index, registerNode]);

  useEffect(() => {
    const node = nodeRef.current;
    if (!node) return;
    const domText = normalizeNewlines(node.innerText);
    if (domText === text) return;
    if (document.activeElement === node) return;
    node.innerText = text;
  }, [text]);

  const handleInput = useCallback(() => {
    const node = nodeRef.current;
    if (!node) return;
    onChange(chunk.index, normalizeNewlines(node.innerText));
  }, [chunk.index, onChange]);

  const handlePaste = useCallback((event: ClipboardEvent<HTMLSpanElement>) => {
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
    <span
      ref={nodeRef}
      className={chunkPresentationClass(chunk)}
      contentEditable={!busy}
      suppressContentEditableWarning
      spellCheck={false}
      role="textbox"
      aria-label={`编辑片段 ${chunk.index + 1}`}
      data-chunk-index={chunk.index + 1}
      onInput={handleInput}
      onPaste={handlePaste}
    >
      {text}
    </span>
  );
});

export const DocxChunkEditor = memo(
  forwardRef<DocumentEditorHandle, DocumentEditorProps>(function DocxChunkEditor(
    {
      session,
      chunkOverrides,
      dirty,
      busy,
      onChange,
      onChangeChunkText,
      onSave,
      onSelectionChange
    },
    ref
  ) {
    const chunkNodesRef = useRef<Record<number, HTMLSpanElement | null>>({});
    const hasSelectionRef = useRef(false);

    const registerNode = useCallback((index: number, node: HTMLSpanElement | null) => {
      chunkNodesRef.current[index] = node;
    }, []);

    const captureChunkSelection = useCallback(() => {
      const selection = window.getSelection();
      const range = selection?.rangeCount ? selection.getRangeAt(0) : null;
      if (!range) return null;

      for (const chunk of session.chunks) {
        if (chunk.skipRewrite) continue;
        const node = chunkNodesRef.current[chunk.index];
        if (!node) continue;
        if (!node.contains(range.startContainer) || !node.contains(range.endContainer)) {
          continue;
        }
        return buildChunkSelectionSnapshot(node, chunk.index, range);
      }

      return null;
    }, [session.chunks]);

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
      const firstEditable = session.chunks.find((chunk) => !chunk.skipRewrite);
      if (!firstEditable) return;
      chunkNodesRef.current[firstEditable.index]?.focus();
    }, [session.chunks]);

    useEffect(() => {
      if (!onSelectionChange) return;

      const handleSelectionChange = () => {
        const next = captureChunkSelection() != null;
        if (next === hasSelectionRef.current) return;
        hasSelectionRef.current = next;
        onSelectionChange(next);
      };

      document.addEventListener("selectionchange", handleSelectionChange);
      return () => document.removeEventListener("selectionchange", handleSelectionChange);
    }, [captureChunkSelection, onSelectionChange]);

    const previewSelectionReplacement = useCallback(
      (
        snapshot: DocumentEditorSelectionSnapshot,
        replacementText: string
      ): DocumentEditorPreviewResult => {
        if (snapshot.kind !== "chunk") {
          return { ok: false, error: "请在单个可编辑片段内重新选中后再试。" };
        }

        const chunk = session.chunks.find((item) => item.index === snapshot.chunkIndex);
        if (!chunk || chunk.skipRewrite) {
          return { ok: false, error: "当前选区不在可编辑片段内，请重新选中后再试。" };
        }

        const currentText = resolveEditorChunkText(chunk, chunkOverrides);
        const replaced = replaceChunkSelectionText(currentText, snapshot, replacementText);
        if (!replaced.ok) return replaced;

        const nextOverrides = applyEditorChunkOverride(chunkOverrides, chunk, replaced.text);
        return {
          ok: true,
          value: buildEditorTextFromChunks(session.chunks, nextOverrides),
          chunkEdits: buildEditorChunkEdits(session.chunks, nextOverrides)
        };
      },
      [chunkOverrides, session.chunks]
    );

    useImperativeHandle(
      ref,
      (): DocumentEditorHandle => ({
        captureSelection: captureChunkSelection,
        previewSelectionReplacement,
        applySelectionReplacement: (snapshot, replacementText) => {
          const preview = previewSelectionReplacement(snapshot, replacementText);
          if (!preview.ok) return preview;
          if (snapshot.kind !== "chunk") {
            return { ok: false, error: "请在单个可编辑片段内重新选中后再试。" };
          }

          const chunk = session.chunks.find((item) => item.index === snapshot.chunkIndex);
          if (!chunk || chunk.skipRewrite) {
            return { ok: false, error: "当前选区不在可编辑片段内，请重新选中后再试。" };
          }

          const currentText = resolveEditorChunkText(chunk, chunkOverrides);
          const replaced = replaceChunkSelectionText(currentText, snapshot, replacementText);
          if (!replaced.ok) return replaced;

          const node = chunkNodesRef.current[chunk.index];
          if (node) {
            node.innerText = replaced.text;
            node.focus();
          }
          onChangeChunkText(chunk.index, replaced.text);
          onChange(preview.value);
          return { ok: true };
        },
        collectChunkEdits: () => buildEditorChunkEdits(session.chunks, chunkOverrides)
      }),
      [captureChunkSelection, chunkOverrides, onChange, onChangeChunkText, previewSelectionReplacement, session.chunks]
    );

    return (
      <div className="workbench-editor-editable docx-editor-flow" aria-label="编辑终稿">
        {session.chunks.map((chunk) => {
          const text = resolveEditorChunkText(chunk, chunkOverrides);
          return (
            <Fragment key={chunk.index}>
              {chunk.skipRewrite ? (
                <span className={chunkPresentationClass(chunk)}>{text}</span>
              ) : (
                <EditableChunkSpan
                  chunk={chunk}
                  text={text}
                  busy={busy}
                  registerNode={registerNode}
                  onChange={onChangeChunkText}
                />
              )}
              {chunk.separatorAfter}
            </Fragment>
          );
        })}
      </div>
    );
  })
);
