import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ChunkTask, EditSuggestion } from "../../../lib/types";
import type { DiffSpan } from "../../../lib/types";
import { summarizeChunkSuggestions } from "../../../lib/helpers";

export type DocumentView = "markup" | "source" | "final";

type CopyState = "idle" | "copying" | "done" | "error";

function buildCopyText(options: {
  chunks: ChunkTask[];
  editorMode: boolean;
  editorText: string;
  documentView: DocumentView;
  suggestionsByChunk: Map<number, EditSuggestion[]>;
}) {
  const { chunks, editorMode, editorText, documentView, suggestionsByChunk } = options;

  if (editorMode) return editorText;
  if (documentView !== "source" && documentView !== "final") return null;

  return chunks
    .map((chunk) => {
      if (documentView === "source") {
        return `${chunk.sourceText}${chunk.separatorAfter}`;
      }

      const chunkSuggestions = suggestionsByChunk.get(chunk.index) ?? [];
      const summary = summarizeChunkSuggestions(chunkSuggestions);
      const displaySuggestion = summary.applied ?? summary.proposed ?? null;
      const body = displaySuggestion ? displaySuggestion.afterText : chunk.sourceText;
      return `${body}${chunk.separatorAfter}`;
    })
    .join("");
}

async function writeClipboardText(text: string) {
  if (typeof navigator !== "undefined" && navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(text);
    return;
  }

  const textarea = document.createElement("textarea");
  textarea.value = text;
  textarea.setAttribute("readonly", "true");
  textarea.style.position = "fixed";
  textarea.style.left = "-9999px";
  textarea.style.top = "0";
  textarea.style.opacity = "0";
  document.body.appendChild(textarea);
  textarea.select();
  textarea.setSelectionRange(0, textarea.value.length);

  const ok = document.execCommand("copy");
  document.body.removeChild(textarea);

  if (!ok) {
    throw new Error("复制失败：浏览器拒绝写入剪贴板。");
  }
}

export function useCopyDocument(options: {
  editorMode: boolean;
  editorText: string;
  documentView: DocumentView;
  chunks: ChunkTask[] | null;
  suggestionsByChunk: Map<number, EditSuggestion[]>;
}) {
  const { editorMode, editorText, documentView, chunks, suggestionsByChunk } = options;

  const [copyState, setCopyState] = useState<CopyState>("idle");
  const copyResetTimerRef = useRef<number | null>(null);

  useEffect(() => {
    return () => {
      if (copyResetTimerRef.current != null) {
        window.clearTimeout(copyResetTimerRef.current);
        copyResetTimerRef.current = null;
      }
    };
  }, []);

  const copyText = useMemo(() => {
    if (!chunks) return null;
    return buildCopyText({
      chunks,
      editorMode,
      editorText,
      documentView,
      suggestionsByChunk
    });
  }, [chunks, documentView, editorMode, editorText, suggestionsByChunk]);

  const canCopy = copyText != null;

  const copyTitle = useMemo(() => {
    if (editorMode) return "复制当前编辑内容";
    if (documentView === "source") return "复制修改前全文";
    if (documentView === "final") return "复制修改后全文";
    return "切换到「修改前 / 修改后」后可复制";
  }, [documentView, editorMode]);

  const handleCopyDocument = useCallback(async () => {
    if (copyText == null) return;

    try {
      setCopyState("copying");
      await writeClipboardText(copyText);
      setCopyState("done");

      if (copyResetTimerRef.current != null) {
        window.clearTimeout(copyResetTimerRef.current);
      }
      copyResetTimerRef.current = window.setTimeout(() => {
        setCopyState("idle");
      }, 1200);
    } catch (error) {
      console.error(error);
      setCopyState("error");
      if (copyResetTimerRef.current != null) {
        window.clearTimeout(copyResetTimerRef.current);
      }
      copyResetTimerRef.current = window.setTimeout(() => {
        setCopyState("idle");
      }, 1600);
    }
  }, [copyText]);

  return { canCopy, copyState, copyTitle, handleCopyDocument };
}

export type { CopyState, DiffSpan };

