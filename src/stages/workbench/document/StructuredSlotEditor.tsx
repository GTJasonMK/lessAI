import {
  forwardRef,
  memo,
  useCallback,
  useEffect,
  useImperativeHandle,
  useRef
} from "react";

import {
  applyEditorSlotOverride,
  buildEditorSlotEdits,
  buildEditorTextFromSession,
  resolveEditorSlotText
} from "../../../lib/editorSlots";
import { normalizeNewlines } from "../../../lib/helpers";
import type {
  DocumentEditorHandle,
  DocumentEditorProps,
  DocumentEditorPreviewResult,
  DocumentEditorSelectionSnapshot,
  SlotSelectionSnapshot
} from "./documentEditorTypes";
import { buildSelectionSnapshotBase } from "./editorSelectionShared";
import { StructuredEditorUnit } from "./StructuredEditorUnit";

function buildSlotSelectionSnapshot(
  node: HTMLElement,
  slotId: string,
  range: Range
): SlotSelectionSnapshot | null {
  const base = buildSelectionSnapshotBase(node, range);
  if (!base) return null;

  return {
    kind: "slot",
    slotId,
    ...base
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

export const StructuredSlotEditor = memo(
  forwardRef<DocumentEditorHandle, DocumentEditorProps>(function StructuredSlotEditor(
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

    const findSessionSlot = useCallback(
      (slotId: string) => session.writebackSlots.find((item) => item.id === slotId) ?? null,
      [session.writebackSlots]
    );

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

        const slot = findSessionSlot(snapshot.slotId);
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
      [findSessionSlot, session, slotOverrides]
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

          const slot = findSessionSlot(snapshot.slotId);
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
      [
        captureSlotSelection,
        findSessionSlot,
        onChange,
        onChangeSlotText,
        previewSelectionReplacement,
        session,
        slotOverrides
      ]
    );

    return (
      <div className="workbench-editor-editable structured-editor-flow" aria-label="编辑终稿">
        {session.rewriteUnits.map((rewriteUnit) => {
          return (
            <StructuredEditorUnit
              key={rewriteUnit.id}
              session={session}
              rewriteUnit={rewriteUnit}
              slotOverrides={slotOverrides}
              busy={busy}
              registerNode={registerNode}
              onChangeSlotText={onChangeSlotText}
            />
          );
        })}
      </div>
    );
  })
);
