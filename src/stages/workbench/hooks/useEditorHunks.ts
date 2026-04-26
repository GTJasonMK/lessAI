import { useDeferredValue, useEffect, useMemo, useState } from "react";
import type { DiffHunk } from "../../../lib/diff";
import { diffTextByLines } from "../../../lib/diff";
import { countCharacters, normalizeNewlines, rewriteUnitSourceText } from "../../../lib/helpers";
import type { DocumentSession } from "../../../lib/types";
import type { EditorSlotOverrides } from "../../../lib/editorSlots";
import { resolveEditorSlotText } from "../../../lib/editorSlots";

function splitLeadingWhitespaceWithNewline(text: string) {
  if (!text) return { leading: "", rest: "" };
  let index = 0;
  let hasNewline = false;
  while (index < text.length) {
    const char = text[index];
    if (char === "\n" || char === "\r") {
      hasNewline = true;
      index += 1;
      continue;
    }
    if (char === " " || char === "\t") {
      index += 1;
      continue;
    }
    break;
  }

  if (!hasNewline) {
    return { leading: "", rest: text };
  }

  return { leading: text.slice(0, index), rest: text.slice(index) };
}

export function useEditorHunks(options: {
  enabled: boolean;
  currentSession: DocumentSession | null;
  editorText: string;
  editorSlotOverrides: EditorSlotOverrides;
}) {
  const { enabled, currentSession, editorText, editorSlotOverrides } = options;
  const deferredEditorText = useDeferredValue(editorText);
  const [activeEditorHunkId, setActiveEditorHunkId] = useState<string | null>(null);

  const slotBasedMode = useMemo(
    () => enabled && currentSession?.capabilities.editorMode === "slotBased",
    [currentSession?.capabilities.editorMode, enabled]
  );

  const slotBasedHunks = useMemo<DiffHunk[]>(() => {
    if (!enabled || !slotBasedMode || !currentSession) return [];

    const changed: DiffHunk[] = [];
    for (const slot of currentSession.writebackSlots) {
      if (!slot.editable) continue;

      const beforeText = normalizeNewlines(slot.text);
      const afterText = normalizeNewlines(resolveEditorSlotText(slot, editorSlotOverrides));
      if (beforeText === afterText) continue;

      const diffSpans = diffTextByLines(beforeText, afterText);
      let insertedChars = 0;
      let deletedChars = 0;
      for (const span of diffSpans) {
        if (span.type === "insert") insertedChars += countCharacters(span.text);
        if (span.type === "delete") deletedChars += countCharacters(span.text);
      }

      changed.push({
        id: `slot-${slot.id}`,
        sequence: changed.length + 1,
        diffSpans,
        beforeText,
        afterText,
        insertedChars,
        deletedChars
      });
    }

    return changed;
  }, [currentSession, editorSlotOverrides, enabled, slotBasedMode]);

  const editorBaselineUnits = useMemo(() => {
    if (!enabled || !currentSession || slotBasedMode) return [];
    return currentSession.rewriteUnits.map((rewriteUnit) => ({
      id: rewriteUnit.id,
      beforeText: normalizeNewlines(rewriteUnitSourceText(currentSession, rewriteUnit))
    }));
  }, [currentSession, enabled, slotBasedMode]);

  const editorBaselineText = useMemo(() => {
    if (!enabled || !currentSession) return "";
    if (slotBasedMode) {
      return normalizeNewlines(currentSession.sourceText);
    }
    return editorBaselineUnits.map((item) => item.beforeText).join("");
  }, [currentSession, editorBaselineUnits, enabled, slotBasedMode]);

  const textUnchanged = useMemo(() => {
    if (!enabled) return true;
    return editorBaselineText === deferredEditorText;
  }, [deferredEditorText, editorBaselineText, enabled]);

  const editorDiffSpans = useMemo(() => {
    if (!enabled || !currentSession || slotBasedMode) return [];
    if (textUnchanged) {
      return [{ type: "unchanged", text: editorBaselineText }];
    }
    return diffTextByLines(editorBaselineText, deferredEditorText);
  }, [
    currentSession,
    deferredEditorText,
    editorBaselineText,
    enabled,
    slotBasedMode,
    textUnchanged
  ]);

  const editorDiffStats = useMemo(() => {
    if (slotBasedMode) {
      return slotBasedHunks.reduce(
        (acc, hunk) => ({
          inserted: acc.inserted + hunk.insertedChars,
          deleted: acc.deleted + hunk.deletedChars
        }),
        { inserted: 0, deleted: 0 }
      );
    }

    let inserted = 0;
    let deleted = 0;
    for (const span of editorDiffSpans) {
      if (span.type === "insert") inserted += countCharacters(span.text);
      if (span.type === "delete") deleted += countCharacters(span.text);
    }
    return { inserted, deleted };
  }, [editorDiffSpans, slotBasedHunks, slotBasedMode]);

  const editorHunks = useMemo<DiffHunk[]>(() => {
    if (slotBasedMode) {
      return slotBasedHunks;
    }

    if (!enabled || !currentSession || editorBaselineUnits.length === 0) {
      return [];
    }
    if (textUnchanged) {
      return [];
    }

    const beforeUnits = editorBaselineUnits.map((item) => item.beforeText);
    const afterUnits = Array.from({ length: beforeUnits.length }, () => "");

    let cursorUnitIndex = 0;
    let cursorOffsetInUnit = 0;

    const advanceUnitForConsumption = () => {
      while (
        cursorUnitIndex < beforeUnits.length &&
        cursorOffsetInUnit === beforeUnits[cursorUnitIndex].length
      ) {
        cursorUnitIndex += 1;
        cursorOffsetInUnit = 0;
      }
    };

    const consumeBeforeText = (text: string, appendToAfter: boolean) => {
      let remaining = text;

      while (remaining.length > 0) {
        advanceUnitForConsumption();
        if (cursorUnitIndex >= beforeUnits.length) {
          if (!appendToAfter) {
            return;
          }
          afterUnits[beforeUnits.length - 1] += remaining;
          return;
        }

        const unitText = beforeUnits[cursorUnitIndex];
        const available = unitText.length - cursorOffsetInUnit;
        if (available <= 0) {
          cursorUnitIndex += 1;
          cursorOffsetInUnit = 0;
          continue;
        }

        const take = Math.min(remaining.length, available);
        const slice = remaining.slice(0, take);
        if (appendToAfter) {
          afterUnits[cursorUnitIndex] += slice;
        }
        cursorOffsetInUnit += take;
        remaining = remaining.slice(take);
      }
    };

    const appendInsert = (text: string) => {
      if (beforeUnits.length === 0) return;

      advanceUnitForConsumption();

      if (cursorUnitIndex >= beforeUnits.length) {
        afterUnits[beforeUnits.length - 1] += text;
        return;
      }

      if (cursorOffsetInUnit === 0 && cursorUnitIndex > 0) {
        const { leading, rest } = splitLeadingWhitespaceWithNewline(text);
        if (leading) {
          afterUnits[cursorUnitIndex - 1] += leading;
        }
        if (rest) {
          afterUnits[cursorUnitIndex] += rest;
        }
        return;
      }

      afterUnits[cursorUnitIndex] += text;
    };

    for (const span of editorDiffSpans) {
      if (span.type === "unchanged") {
        consumeBeforeText(span.text, true);
        continue;
      }
      if (span.type === "delete") {
        consumeBeforeText(span.text, false);
        continue;
      }
      appendInsert(span.text);
    }

    const changed: DiffHunk[] = [];

    for (let index = 0; index < beforeUnits.length; index += 1) {
      const beforeText = beforeUnits[index];
      const afterText = afterUnits[index] ?? "";
      if (beforeText === afterText) continue;

      const diffSpans = diffTextByLines(beforeText, afterText);
      let insertedChars = 0;
      let deletedChars = 0;
      for (const span of diffSpans) {
        if (span.type === "insert") insertedChars += countCharacters(span.text);
        if (span.type === "delete") deletedChars += countCharacters(span.text);
      }

      const sequence = changed.length + 1;
      const rewriteUnitId = editorBaselineUnits[index]?.id ?? `rewrite-unit-${index}`;
      changed.push({
        id: `rewrite-unit-${rewriteUnitId}`,
        sequence,
        diffSpans,
        beforeText,
        afterText,
        insertedChars,
        deletedChars
      });
    }

    return changed;
  }, [
    currentSession,
    editorBaselineUnits,
    editorDiffSpans,
    enabled,
    slotBasedHunks,
    slotBasedMode,
    textUnchanged
  ]);

  const activeEditorHunk = useMemo(() => {
    if (!enabled || editorHunks.length === 0) return null;
    return editorHunks.find((item) => item.id === activeEditorHunkId) ?? editorHunks[0];
  }, [activeEditorHunkId, editorHunks, enabled]);

  useEffect(() => {
    if (!enabled) {
      if (activeEditorHunkId !== null) setActiveEditorHunkId(null);
      return;
    }
    if (editorHunks.length === 0) {
      if (activeEditorHunkId !== null) {
        setActiveEditorHunkId(null);
      }
      return;
    }
    if (!activeEditorHunkId || !editorHunks.some((item) => item.id === activeEditorHunkId)) {
      setActiveEditorHunkId(editorHunks[0].id);
    }
  }, [activeEditorHunkId, editorHunks, enabled]);

  return {
    editorBaselineText,
    editorDiffStats,
    editorHunks,
    activeEditorHunk,
    activeEditorHunkId,
    setActiveEditorHunkId
  };
}
