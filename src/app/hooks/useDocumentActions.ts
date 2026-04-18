import { open } from "@tauri-apps/plugin-dialog";
import { startTransition, useCallback } from "react";
import { openDocument, runDocumentWriteback } from "../../lib/api";
import { buildEditorSlotEdits, buildEditorTextFromSession } from "../../lib/editorSlots";
import type { DocumentSession, DocumentSnapshot, RewriteProgress } from "../../lib/types";
import {
  isDocxPath,
  isPdfPath,
  normalizeNewlines,
  readableError
} from "../../lib/helpers";
import {
  applyUpdatedSessionState,
  runSessionActionWithScroll,
  refreshAllowedSessionOrNotify,
  type ApplySessionState,
  type RefreshSessionState,
  type ShowNotice,
  type WithBusy
} from "./sessionActionShared";
import { logScrollRestore } from "./documentScrollRestoreDebug";

export function useDocumentActions(options: {
  busyAction: string | null;
  stageRef: React.MutableRefObject<"workbench" | "editor">;
  currentSessionRef: React.MutableRefObject<DocumentSession | null>;
  activeRewriteUnitIdRef: React.MutableRefObject<string | null>;
  captureDocumentScrollPosition: () => number | null;
  editorDirtyRef: React.MutableRefObject<boolean>;
  editorTextRef: React.MutableRefObject<string>;
  editorBaselineTextRef: React.MutableRefObject<string>;
  editorBaseSnapshotRef: React.MutableRefObject<DocumentSnapshot | null>;
  editorSlotOverridesRef: React.MutableRefObject<Record<string, string>>;
  applySessionState: ApplySessionState;
  refreshSessionState: RefreshSessionState;
  setStage: React.Dispatch<React.SetStateAction<"workbench" | "editor">>;
  setEditorBaselineText: React.Dispatch<React.SetStateAction<string>>;
  setEditorText: React.Dispatch<React.SetStateAction<string>>;
  setEditorSlotOverrides: React.Dispatch<React.SetStateAction<Record<string, string>>>;
  setLiveProgress: React.Dispatch<React.SetStateAction<RewriteProgress | null>>;
  setSettingsOpen: React.Dispatch<React.SetStateAction<boolean>>;
  closeSettings: () => void;
  showNotice: ShowNotice;
  withBusy: WithBusy;
}) {
  const {
    busyAction,
    stageRef,
    currentSessionRef,
    activeRewriteUnitIdRef,
    captureDocumentScrollPosition,
    editorDirtyRef,
    editorTextRef,
    editorBaselineTextRef,
    editorBaseSnapshotRef,
    editorSlotOverridesRef,
    applySessionState,
    refreshSessionState,
    setStage,
    setEditorBaselineText,
    setEditorText,
    setEditorSlotOverrides,
    setLiveProgress,
    setSettingsOpen,
    closeSettings,
    showNotice,
    withBusy
  } = options;

  const handleOpenDocument = useCallback(async () => {
    if (stageRef.current === "editor") {
      showNotice(
        "warning",
        editorDirtyRef.current
          ? "你有未保存的手动编辑，请先保存或放弃修改。"
          : "请先返回工作台后再打开其他文件。"
      );
      return;
    }

    const session = currentSessionRef.current;
    if (session && ["running", "paused"].includes(session.status)) {
      showNotice("warning", "当前文档正在执行自动任务，请先取消或等待完成后再打开其他文件。");
      return;
    }

    try {
      const selection = await open({
        multiple: false,
        directory: false,
        filters: [
          {
            name: "Documents",
            extensions: ["txt", "md", "markdown", "tex", "latex", "docx", "pdf"]
          }
        ]
      });
      if (!selection) return;

      const path = Array.isArray(selection) ? selection[0] : selection;
      if (!path) return;

      const opened = await withBusy("open-document", () => openDocument(path));
      applyUpdatedSessionState({ session: opened, applySessionState });
      setStage("workbench");
      setEditorBaselineText("");
      setEditorText("");
      editorBaseSnapshotRef.current = null;
      setEditorSlotOverrides({});
      closeSettings();
      showNotice(
        "success",
        `已打开文档：${opened.title}（共 ${opened.rewriteUnits.length} 段，可继续上次进度）。`
      );
    } catch (error) {
      showNotice("error", `打开失败：${readableError(error)}`);
    }
  }, [
    applySessionState,
    closeSettings,
    currentSessionRef,
    editorDirtyRef,
    editorBaseSnapshotRef,
    setEditorBaselineText,
    setEditorSlotOverrides,
    setEditorText,
    setStage,
    showNotice,
    stageRef,
    withBusy
  ]);

  const handleEnterEditor = useCallback(async () => {
    const session = currentSessionRef.current;
    if (!session) {
      showNotice("warning", "请先打开一个文档。");
      return;
    }

    const latestSession = await refreshAllowedSessionOrNotify({
      session,
      refreshSessionState,
      options: {
        preserveRewriteUnit: true,
        preserveSuggestion: true
      },
      showNotice,
      errorPrefix: "进入编辑模式失败",
      formatError: readableError,
      allowed: (current) => current.plainTextEditorSafe,
      blockedMessage: (current) => current.plainTextEditorBlockReason,
      fallbackMessage: "当前文档暂不支持进入编辑模式。"
    });
    if (!latestSession) {
      return;
    }
    if (isPdfPath(latestSession.documentPath)) {
      showNotice(
        "warning",
        "pdf 目前仅支持导入/改写/导出，暂不支持终稿编辑或写回覆盖。"
      );
      return;
    }

    if (busyAction) {
      showNotice("warning", "当前有操作在执行，请稍后再试。");
      return;
    }

    if (latestSession.status === "running" || latestSession.status === "paused") {
      showNotice("warning", "文档正在执行自动任务，请先取消后再编辑。");
      return;
    }

    const cleanSession =
      latestSession.status === "idle" &&
      latestSession.suggestions.length === 0 &&
      latestSession.rewriteUnits.every(
        (rewriteUnit) => rewriteUnit.status === "idle" || rewriteUnit.status === "done"
      );

    if (!cleanSession) {
      showNotice(
        "warning",
        "该文档存在修订记录或进度，为避免冲突，请先“覆写并清理记录”或“重置记录”后再编辑。"
      );
      return;
    }

    startTransition(() => {
      setStage("editor");
      const baseline = isDocxPath(latestSession.documentPath)
        ? buildEditorTextFromSession(latestSession, {})
        : normalizeNewlines(latestSession.sourceText);
      setEditorSlotOverrides({});
      setEditorBaselineText(baseline);
      setEditorText(baseline);
      setLiveProgress(null);
      setSettingsOpen(false);
    });
    editorBaseSnapshotRef.current = latestSession.sourceSnapshot ?? null;
    if (isDocxPath(latestSession.documentPath)) {
      showNotice(
        "info",
        "docx 编辑模式已按可写回槽位开放：锁定内容保持只读，可编辑范围与 AI 改写和写回范围一致。"
      );
    }
  }, [
    busyAction,
    currentSessionRef,
    editorBaseSnapshotRef,
    refreshSessionState,
    setEditorBaselineText,
    setEditorSlotOverrides,
    setEditorText,
    setLiveProgress,
    setSettingsOpen,
    setStage,
    showNotice
  ]);

  const handleDiscardEditorChanges = useCallback(() => {
    if (stageRef.current !== "editor") return;
    if (!editorDirtyRef.current) {
      showNotice("info", "当前没有需要放弃的修改。");
      return;
    }
    startTransition(() => {
      setEditorSlotOverrides({});
      setEditorText(editorBaselineTextRef.current);
    });
    showNotice("warning", "已放弃未保存的修改。");
  }, [
    editorBaselineTextRef,
    editorDirtyRef,
    setEditorSlotOverrides,
    setEditorText,
    showNotice,
    stageRef
  ]);

  const handleExitEditor = useCallback(() => {
    if (stageRef.current !== "editor") return;
    if (editorDirtyRef.current) {
      showNotice("warning", "你有未保存的手动编辑，请先保存或放弃修改。");
      return;
    }
    editorBaseSnapshotRef.current = null;
    setStage("workbench");
  }, [editorBaseSnapshotRef, editorDirtyRef, setStage, showNotice, stageRef]);

  const handleSaveEditor = useCallback(
    async (options?: { returnToWorkbench?: boolean }) => {
      const session = currentSessionRef.current;
      if (!session) return;
      if (stageRef.current !== "editor") return;

      if (!editorDirtyRef.current) {
        showNotice("info", "没有修改，无需保存。");
        if (options?.returnToWorkbench) {
          editorBaseSnapshotRef.current = null;
          setStage("workbench");
        }
        return;
      }

      const returnToWorkbench = Boolean(options?.returnToWorkbench);
      const actionKey = returnToWorkbench ? "save-edits-and-back" : "save-edits";
      const content = editorTextRef.current;
      const preservedScrollTop = captureDocumentScrollPosition();
      logScrollRestore("editor-save-start", {
        sessionId: session.id,
        actionKey,
        returnToWorkbench,
        preservedScrollTop
      });

      try {
        const {
          session: updated,
          preservedScrollTop: restoredScrollTop
        } = await runSessionActionWithScroll({
          captureDocumentScrollPosition,
          applySessionState,
          preservedScrollTop,
          run: () =>
            withBusy(actionKey, () => {
              if (!isDocxPath(session.documentPath)) {
                return runDocumentWriteback(session.id, "write", { kind: "text", content }, editorBaseSnapshotRef.current);
              }

              const edits = buildEditorSlotEdits(session, editorSlotOverridesRef.current);
              return runDocumentWriteback(session.id, "write", { kind: "slotEdits", edits }, editorBaseSnapshotRef.current);
            }),
          resolveState: () => ({
            preferredRewriteUnitId: activeRewriteUnitIdRef.current
          })
        });
        editorBaseSnapshotRef.current = updated.sourceSnapshot ?? null;
        logScrollRestore("editor-save-restoring", {
          sessionId: session.id,
          updatedSessionId: updated.id,
          preservedScrollTop: restoredScrollTop
        });
        setLiveProgress(null);

        startTransition(() => {
          const baseline = isDocxPath(updated.documentPath)
            ? buildEditorTextFromSession(updated, {})
            : normalizeNewlines(updated.sourceText);
          setEditorSlotOverrides({});
          setEditorBaselineText(baseline);
          setEditorText(baseline);
        });

        if (returnToWorkbench) {
          editorBaseSnapshotRef.current = null;
          setStage("workbench");
          showNotice("success", "已保存并返回工作台，可继续 AI 优化。");
          return;
        }

        showNotice("success", "已保存到原文件。");
      } catch (error) {
        showNotice("error", `保存失败：${readableError(error)}`);
      }
    },
    [
      activeRewriteUnitIdRef,
      applySessionState,
      captureDocumentScrollPosition,
      currentSessionRef,
      editorDirtyRef,
      editorBaseSnapshotRef,
      editorSlotOverridesRef,
      editorTextRef,
      setEditorBaselineText,
      setEditorSlotOverrides,
      setEditorText,
      setLiveProgress,
      setStage,
      showNotice,
      stageRef,
      withBusy
    ]
  );

  return {
    handleOpenDocument,
    handleEnterEditor,
    handleDiscardEditorChanges,
    handleExitEditor,
    handleSaveEditor
  } as const;
}
