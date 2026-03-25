import { useDeferredValue, useEffect, useMemo, useState } from "react";
import type { DiffHunk } from "../../../lib/diff";
import { diffTextByLines } from "../../../lib/diff";
import { countCharacters, normalizeNewlines } from "../../../lib/helpers";
import type { DocumentSession } from "../../../lib/types";

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
}) {
  const { enabled, currentSession, editorText } = options;
  const deferredEditorText = useDeferredValue(editorText);
  const [activeEditorHunkId, setActiveEditorHunkId] = useState<string | null>(null);

  const editorBaselineChunks = useMemo(() => {
    if (!enabled) return [];
    if (!currentSession) return [];
    return currentSession.chunks.map((chunk) => ({
      index: chunk.index,
      beforeText: normalizeNewlines(`${chunk.sourceText}${chunk.separatorAfter}`)
    }));
  }, [currentSession, enabled]);

  const editorBaselineText = useMemo(() => {
    if (!enabled) return "";
    if (!currentSession) return "";
    return editorBaselineChunks.map((item) => item.beforeText).join("");
  }, [currentSession, editorBaselineChunks, enabled]);

  const editorDiffSpans = useMemo(() => {
    if (!enabled) return [];
    if (!currentSession) return [];
    return diffTextByLines(editorBaselineText, deferredEditorText);
  }, [currentSession, deferredEditorText, editorBaselineText, enabled]);

  const editorDiffStats = useMemo(() => {
    let inserted = 0;
    let deleted = 0;
    for (const span of editorDiffSpans) {
      if (span.type === "insert") inserted += countCharacters(span.text);
      if (span.type === "delete") deleted += countCharacters(span.text);
    }
    return { inserted, deleted };
  }, [editorDiffSpans]);

  const editorHunks = useMemo<DiffHunk[]>(() => {
    if (!enabled) return [];
    if (!currentSession) return [];
    if (editorBaselineChunks.length === 0) return [];

    const beforeChunks = editorBaselineChunks.map((item) => item.beforeText);
    const afterChunks = Array.from({ length: beforeChunks.length }, () => "");

    let cursorChunkIndex = 0;
    let cursorOffsetInChunk = 0;

    const advanceChunkForConsumption = () => {
      while (
        cursorChunkIndex < beforeChunks.length &&
        cursorOffsetInChunk === beforeChunks[cursorChunkIndex].length
      ) {
        cursorChunkIndex += 1;
        cursorOffsetInChunk = 0;
      }
    };

    const consumeBeforeText = (text: string, appendToAfter: boolean) => {
      let remaining = text;

      while (remaining.length > 0) {
        advanceChunkForConsumption();
        if (cursorChunkIndex >= beforeChunks.length) {
          if (!appendToAfter) {
            return;
          }
          // 理论上不会发生（before 总长度应与 chunks 相等），这里兜底避免丢失插入的尾部文本。
          afterChunks[beforeChunks.length - 1] += remaining;
          return;
        }

        const chunkText = beforeChunks[cursorChunkIndex];
        const available = chunkText.length - cursorOffsetInChunk;
        if (available <= 0) {
          cursorChunkIndex += 1;
          cursorOffsetInChunk = 0;
          continue;
        }

        const take = Math.min(remaining.length, available);
        const slice = remaining.slice(0, take);
        if (appendToAfter) {
          afterChunks[cursorChunkIndex] += slice;
        }
        cursorOffsetInChunk += take;
        remaining = remaining.slice(take);
      }
    };

    const appendInsert = (text: string) => {
      if (beforeChunks.length === 0) return;

      advanceChunkForConsumption();

      if (cursorChunkIndex >= beforeChunks.length) {
        afterChunks[beforeChunks.length - 1] += text;
        return;
      }

      // 关键：当插入发生在 chunk 起始处时，优先把“行分隔/空白”归属到上一块，
      // 这样在审阅侧的最小 diff 单元（chunk）更稳定，不会把空行和下一段绑定在一起。
      if (cursorOffsetInChunk === 0 && cursorChunkIndex > 0) {
        const { leading, rest } = splitLeadingWhitespaceWithNewline(text);
        if (leading) {
          afterChunks[cursorChunkIndex - 1] += leading;
        }
        if (rest) {
          afterChunks[cursorChunkIndex] += rest;
        }
        return;
      }

      afterChunks[cursorChunkIndex] += text;
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

    for (let index = 0; index < beforeChunks.length; index += 1) {
      const beforeText = beforeChunks[index];
      const afterText = afterChunks[index] ?? "";
      if (beforeText === afterText) continue;

      const diffSpans = diffTextByLines(beforeText, afterText);
      let insertedChars = 0;
      let deletedChars = 0;
      for (const span of diffSpans) {
        if (span.type === "insert") insertedChars += countCharacters(span.text);
        if (span.type === "delete") deletedChars += countCharacters(span.text);
      }

      const sequence = changed.length + 1;
      const chunkIndex = editorBaselineChunks[index]?.index ?? index;
      changed.push({
        id: `chunk-${chunkIndex}`,
        sequence,
        diffSpans,
        beforeText,
        afterText,
        insertedChars,
        deletedChars
      });
    }

    return changed;
  }, [currentSession, editorBaselineChunks, editorDiffSpans, enabled]);

  const activeEditorHunk = useMemo(() => {
    if (!enabled) return null;
    if (editorHunks.length === 0) return null;
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

