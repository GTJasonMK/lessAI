import {
  Fragment,
  forwardRef,
  memo,
  useCallback,
  useEffect,
  useImperativeHandle,
  useLayoutEffect,
  useRef
} from "react";
import type { ClipboardEvent } from "react";

import {
  applyEditorSlotOverride,
  buildEditorSlotEdits,
  buildEditorTextFromSession,
  resolveEditorSlotText,
} from "../../../lib/editorSlots";
import { normalizeNewlines } from "../../../lib/helpers";
import type { WritebackSlot } from "../../../lib/types";
import type {
  DocumentEditorHandle,
  DocumentEditorProps,
  DocumentEditorSelectionSnapshot,
  DocumentEditorPreviewResult,
  SlotSelectionSnapshot,
} from "./documentEditorTypes";

function selectionPointOffset(node: HTMLElement, container: Node, offset: number) {
  const range = document.createRange();
  range.selectNodeContents(node);
  range.setEnd(container, offset);
  return normalizeNewlines(range.toString()).length;
}

function buildSlotSelectionSnapshot(
  node: HTMLElement,
  slotId: string,
  range: Range
): SlotSelectionSnapshot | null {
  if (range.collapsed) return null;
  if (!node.contains(range.startContainer) || !node.contains(range.endContainer)) {
    return null;
  }

  const text = normalizeNewlines(range.toString());
  if (text.trim().length === 0) return null;

  return {
    kind: "slot",
    slotId,
    text,
    startOffset: selectionPointOffset(node, range.startContainer, range.startOffset),
    endOffset: selectionPointOffset(node, range.endContainer, range.endOffset)
  };
}

function replaceSelectionText(
  currentText: string,
  snapshot: SlotSelectionSnapshot,
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

function slotPresentationClass(slot: WritebackSlot) {
  const presentation = slot.presentation;
  return [
    "docx-editor-slot",
    slot.editable ? "is-editable" : "is-locked",
    presentation?.bold ? "is-bold" : "",
    presentation?.italic ? "is-italic" : "",
    presentation?.underline ? "is-underline" : "",
    presentation?.href ? "is-link" : ""
  ]
    .filter(Boolean)
    .join(" ");
}

const EditableSlotSpan = memo(function EditableSlotSpan({
  slot,
  text,
  busy,
  registerNode,
  onChange,
}: {
  slot: WritebackSlot;
  text: string;
  busy: boolean;
  registerNode: (slotId: string, node: HTMLSpanElement | null) => void;
  onChange: (slotId: string, value: string) => void;
}) {
  const nodeRef = useRef<HTMLSpanElement | null>(null);

  useEffect(() => {
    registerNode(slot.id, nodeRef.current);
    return () => registerNode(slot.id, null);
  }, [registerNode, slot.id]);

  useLayoutEffect(() => {
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
    onChange(slot.id, normalizeNewlines(node.innerText));
  }, [onChange, slot.id]);

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
      className={slotPresentationClass(slot)}
      contentEditable={!busy}
      suppressContentEditableWarning
      spellCheck={false}
      role="textbox"
      aria-label={`编辑槽位 ${slot.order + 1}`}
      data-slot-id={slot.id}
      onInput={handleInput}
      onPaste={handlePaste}
    />
  );
});

export const DocxSlotEditor = memo(
  forwardRef<DocumentEditorHandle, DocumentEditorProps>(function DocxSlotEditor(
    {
      session,
      slotOverrides,
      dirty,
      busy,
      onChange,
      onChangeSlotText,
      onSave,
      onSelectionChange
    },
    ref
  ) {
    const slotNodesRef = useRef<Record<string, HTMLSpanElement | null>>({});
    const hasSelectionRef = useRef(false);

    const registerNode = useCallback((slotId: string, node: HTMLSpanElement | null) => {
      slotNodesRef.current[slotId] = node;
    }, []);

    const captureSlotSelection = useCallback(() => {
      const selection = window.getSelection();
      const range = selection?.rangeCount ? selection.getRangeAt(0) : null;
      if (!range) return null;

      for (const slot of session.writebackSlots) {
        if (!slot.editable) continue;
        const node = slotNodesRef.current[slot.id];
        if (!node) continue;
        if (!node.contains(range.startContainer) || !node.contains(range.endContainer)) {
          continue;
        }
        return buildSlotSelectionSnapshot(node, slot.id, range);
      }

      return null;
    }, [session.writebackSlots]);

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
      const firstEditable = session.writebackSlots.find((slot) => slot.editable);
      if (!firstEditable) return;
      slotNodesRef.current[firstEditable.id]?.focus();
    }, [session.id, session.writebackSlots]);

    useEffect(() => {
      if (!onSelectionChange) return;

      const handleSelectionChange = () => {
        const next = captureSlotSelection() != null;
        if (next === hasSelectionRef.current) return;
        hasSelectionRef.current = next;
        onSelectionChange(next);
      };

      document.addEventListener("selectionchange", handleSelectionChange);
      return () => document.removeEventListener("selectionchange", handleSelectionChange);
    }, [captureSlotSelection, onSelectionChange]);

    const previewSelectionReplacement = useCallback(
      (
        snapshot: DocumentEditorSelectionSnapshot,
        replacementText: string
      ): DocumentEditorPreviewResult => {
        if (snapshot.kind !== "slot") {
          return { ok: false, error: "请在单个可编辑片段内重新选中后再试。" };
        }

        const slot = session.writebackSlots.find((item) => item.id === snapshot.slotId);
        if (!slot || !slot.editable) {
          return { ok: false, error: "当前选区不在可编辑片段内，请重新选中后再试。" };
        }

        const currentText = resolveEditorSlotText(slot, slotOverrides);
        const replaced = replaceSelectionText(currentText, snapshot, replacementText);
        if (!replaced.ok) return replaced;

        const nextOverrides = applyEditorSlotOverride(slotOverrides, slot, replaced.text);
        return {
          ok: true,
          value: buildEditorTextFromSession(session, nextOverrides),
          slotEdits: buildEditorSlotEdits(session, nextOverrides)
        };
      },
      [session, slotOverrides]
    );

    useImperativeHandle(
      ref,
      (): DocumentEditorHandle => ({
        captureSelection: captureSlotSelection,
        previewSelectionReplacement,
        applySelectionReplacement: (snapshot, replacementText) => {
          const preview = previewSelectionReplacement(snapshot, replacementText);
          if (!preview.ok) return preview;
          if (snapshot.kind !== "slot") {
            return { ok: false, error: "请在单个可编辑片段内重新选中后再试。" };
          }

          const slot = session.writebackSlots.find((item) => item.id === snapshot.slotId);
          if (!slot || !slot.editable) {
            return { ok: false, error: "当前选区不在可编辑片段内，请重新选中后再试。" };
          }

          const currentText = resolveEditorSlotText(slot, slotOverrides);
          const replaced = replaceSelectionText(currentText, snapshot, replacementText);
          if (!replaced.ok) return replaced;

          const node = slotNodesRef.current[slot.id];
          if (node) {
            node.innerText = replaced.text;
            node.focus();
          }
          onChangeSlotText(slot.id, replaced.text);
          onChange(preview.value);
          return { ok: true };
        },
        collectSlotEdits: () => buildEditorSlotEdits(session, slotOverrides)
      }),
      [captureSlotSelection, onChange, onChangeSlotText, previewSelectionReplacement, session, slotOverrides]
    );

    return (
      <div className="workbench-editor-editable docx-editor-flow" aria-label="编辑终稿">
        {session.writebackSlots.map((slot) => {
          const text = resolveEditorSlotText(slot, slotOverrides);
          return (
            <Fragment key={slot.id}>
              {slot.editable ? (
                <EditableSlotSpan
                  slot={slot}
                  text={text}
                  busy={busy}
                  registerNode={registerNode}
                  onChange={onChangeSlotText}
                />
              ) : (
                <span className={slotPresentationClass(slot)}>{text}</span>
              )}
              {slot.separatorAfter}
            </Fragment>
          );
        })}
      </div>
    );
  })
);
