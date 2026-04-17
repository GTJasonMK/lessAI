import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { DiffSpan, DocumentSession, RewriteSuggestion } from "../../../lib/types";
import {
  groupSuggestionsByRewriteUnit,
  rewriteUnitSourceText,
  summarizeRewriteUnitSuggestions
} from "../../../lib/helpers";

export type DocumentView = "markup" | "source" | "final";

type CopyState = "idle" | "copying" | "done" | "error";

function buildCopyText(options: {
  currentSession: DocumentSession;
  editorMode: boolean;
  editorText: string;
  documentView: DocumentView;
  suggestionsByRewriteUnit: Map<string, RewriteSuggestion[]>;
}) {
  const { currentSession, editorMode, editorText, documentView, suggestionsByRewriteUnit } = options;

  if (editorMode) return editorText;
  if (documentView !== "source" && documentView !== "final") return null;

  return currentSession.rewriteUnits
    .map((rewriteUnit) => {
      const source = rewriteUnitSourceText(currentSession, rewriteUnit);
      if (documentView === "source") {
        return source;
      }

      const unitSuggestions = suggestionsByRewriteUnit.get(rewriteUnit.id) ?? [];
      const summary = summarizeRewriteUnitSuggestions(unitSuggestions);
      const displaySuggestion = summary.applied ?? summary.proposed ?? null;
      return displaySuggestion?.afterText ?? source;
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
  currentSession: DocumentSession | null;
}) {
  const { editorMode, editorText, documentView, currentSession } = options;

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

  const suggestionsByRewriteUnit = useMemo(
    () => groupSuggestionsByRewriteUnit(currentSession?.suggestions ?? []),
    [currentSession?.suggestions]
  );

  const copyText = useMemo(() => {
    if (!currentSession) return null;
    return buildCopyText({
      currentSession,
      editorMode,
      editorText,
      documentView,
      suggestionsByRewriteUnit
    });
  }, [currentSession, documentView, editorMode, editorText, suggestionsByRewriteUnit]);

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
