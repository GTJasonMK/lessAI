import { startTransition, useCallback } from "react";
import {
  exportDocument,
  finalizeDocument,
  openDocument,
  resetSession
} from "../../lib/api";
import { saveRuntimeDialog } from "../../lib/runtimeDialog";
import { isDemoRuntime } from "../../lib/runtimeMode";
import {
  documentBackendKind,
  sessionSupportsSourceWriteback
} from "../../lib/documentCapabilities";
import type { DocumentSession, RewriteProgress } from "../../lib/types";
import {
  formatDisplayPath,
  getSessionStats,
  readableError,
  sanitizeFileName
} from "../../lib/helpers";
import type { ConfirmModalOptions } from "../../components/ConfirmModal";
import {
  refreshAllowedSessionOrNotify,
  restoreLoadedSessionWithScroll,
  runSessionActionOrNotify,
  type ApplySessionState,
  type RefreshSessionState,
  type ShowNotice,
  type WithBusy
} from "./sessionActionShared";
import { logScrollRestore } from "./documentScrollRestoreDebug";

export function useDocumentFinalizeActions(options: {
  stageRef: React.MutableRefObject<"workbench" | "editor">;
  currentSessionRef: React.MutableRefObject<DocumentSession | null>;
  activeRewriteUnitIdRef: React.MutableRefObject<string | null>;
  editorDirtyRef: React.MutableRefObject<boolean>;
  captureDocumentScrollPosition: () => number | null;
  requestConfirm: (options: ConfirmModalOptions) => Promise<boolean>;
  applySessionState: ApplySessionState;
  refreshSessionState: RefreshSessionState;
  setCurrentSession: React.Dispatch<React.SetStateAction<DocumentSession | null>>;
  setActiveRewriteUnitId: React.Dispatch<React.SetStateAction<string | null>>;
  setActiveSuggestionId: React.Dispatch<React.SetStateAction<string | null>>;
  setLiveProgress: React.Dispatch<React.SetStateAction<RewriteProgress | null>>;
  closeSettings: () => void;
  showNotice: ShowNotice;
  withBusy: WithBusy;
}) {
  const demoRuntime = isDemoRuntime();
  const {
    stageRef,
    currentSessionRef,
    activeRewriteUnitIdRef,
    editorDirtyRef,
    captureDocumentScrollPosition,
    requestConfirm,
    applySessionState,
    refreshSessionState,
    setCurrentSession,
    setActiveRewriteUnitId,
    setActiveSuggestionId,
    setLiveProgress,
    closeSettings,
    showNotice,
    withBusy
  } = options;

  const handleExport = useCallback(async () => {
    if (stageRef.current === "editor") {
      showNotice(
        "warning",
        editorDirtyRef.current
          ? "你有未保存的手动编辑，请先保存或放弃修改后再导出。"
          : "请先返回工作台后再导出终稿。"
      );
      return;
    }

    const session = currentSessionRef.current;
    if (!session) {
      showNotice("warning", "当前没有可导出的文档。");
      return;
    }
    if (session.status === "running" || session.status === "paused") {
      showNotice("warning", "文档正在执行自动任务，请先暂停并取消后再导出。");
      return;
    }
    try {
      const path = await saveRuntimeDialog({
        defaultPath: `${sanitizeFileName(session.title)}.txt`,
        filters: [{ name: "Text", extensions: ["txt"] }]
      });
      if (!path) return;
      const savedPath = await withBusy("export-document", () => exportDocument(session.id, path));
      showNotice("success", `已导出到 ${formatDisplayPath(savedPath)}`);
    } catch (error) {
      showNotice("error", `导出失败：${readableError(error)}`);
    }
  }, [currentSessionRef, editorDirtyRef, showNotice, stageRef, withBusy]);

  const handleFinalizeDocument = useCallback(async () => {
    const session = currentSessionRef.current;
    if (!session) {
      showNotice("warning", demoRuntime ? "当前没有可保存的文档。" : "当前没有可写回的文档。");
      return;
    }
    const actionLabel = demoRuntime ? "保存" : "写回";
    const latestSession = await refreshAllowedSessionOrNotify({
      session,
      refreshSessionState,
      options: {
        preserveRewriteUnit: true,
        preserveSuggestion: true
      },
      showNotice,
      errorPrefix: `${actionLabel}失败`,
      formatError: readableError,
      allowed: sessionSupportsSourceWriteback,
      blockedMessage: (current) => current.capabilities.sourceWriteback.blockReason,
      defaultBlockedMessage: demoRuntime
        ? "当前文档暂不支持保存到网页缓存。"
        : "当前文档暂不支持安全写回覆盖。"
    });
    if (!latestSession) {
      return;
    }

    if (latestSession.status === "running" || latestSession.status === "paused") {
      showNotice(
        "warning",
        demoRuntime
          ? "文档正在执行自动任务，请先取消后再保存到网页缓存。"
          : "文档正在执行自动任务，请先取消后再写回原文件。"
      );
      return;
    }

    const stats = getSessionStats(latestSession);
    const hints = demoRuntime
      ? [
        "该操作会把【已应用】的修改保存到网页缓存，并删除该文档的全部历史记录（建议、进度）。",
        "不会覆盖本地文件，建议你先“导出”做一份下载备份。",
        "保存成功后会自动重新打开该网页文档缓存（以全新会话展示）。",
        "",
        `文件：${formatDisplayPath(latestSession.documentPath)}`,
        `已应用：${stats.unitsApplied}/${stats.total}`,
        stats.unitsProposed > 0
          ? `注意：仍有 ${stats.unitsProposed} 段待处理，不会写入缓存。`
          : "待处理：0（将完整保存已应用结果）",
        stats.pendingGeneration > 0
          ? `注意：仍有 ${stats.pendingGeneration} 段未生成/失败，保存时会保留原文。`
          : "未生成：0"
      ]
      : [
        "该操作会把【已应用】的修改覆盖写回原文件，并删除该文档的全部历史记录（建议、进度）。",
        "不可撤销，建议你先“导出”做一份备份。",
        "写回成功后会自动重新打开该文件（以全新会话展示）。",
        ["docx", "pdf"].includes(documentBackendKind(latestSession))
          ? "当前文档采用安全写回子集：复杂结构会锁定保留；若结构一致性不足，系统会拒绝覆盖以避免写坏原文件。"
          : "",
        "",
        `文件：${formatDisplayPath(latestSession.documentPath)}`,
        `已应用：${stats.unitsApplied}/${stats.total}`,
        stats.unitsProposed > 0
          ? `注意：仍有 ${stats.unitsProposed} 段待处理，不会写入文件。`
          : "待处理：0（将完整写回已应用结果）",
        stats.pendingGeneration > 0
          ? `注意：仍有 ${stats.pendingGeneration} 段未生成/失败，写回时会保留原文。`
          : "未生成：0"
      ];

    const ok = await requestConfirm({
      title: demoRuntime ? "保存到网页缓存并清理记录" : "覆盖原文件并清理记录",
      message: hints.join("\n"),
      okLabel: demoRuntime ? "保存并清理" : "覆盖并清理",
      cancelLabel: "取消",
      variant: "danger"
    });

    if (!ok) return;

    let savedPath: string | null = null;
    const preservedScrollTop = captureDocumentScrollPosition();
    logScrollRestore("finalize-start", {
      sessionId: latestSession.id,
      preservedScrollTop,
      path: latestSession.documentPath
    });
    try {
      savedPath = await withBusy("finalize-document", () => finalizeDocument(latestSession.id));
      const reopened = await openDocument(savedPath);
      await restoreLoadedSessionWithScroll({
        captureDocumentScrollPosition,
        applySessionState,
        session: reopened,
        preservedScrollTop,
        preferredRewriteUnitId: activeRewriteUnitIdRef.current
      });

      logScrollRestore("finalize-restoring", {
        previousSessionId: latestSession.id,
        reopenedSessionId: reopened.id,
        preservedScrollTop,
        savedPath
      });
      setLiveProgress(null);
      closeSettings();
      showNotice(
        "success",
        demoRuntime
          ? `已保存到网页缓存并清理记录，并重新打开：${savedPath ? formatDisplayPath(savedPath) : ""}`
          : `已覆盖并清理，并重新打开：${savedPath ? formatDisplayPath(savedPath) : ""}`
      );
    } catch (error) {
      if (savedPath) {
        startTransition(() => {
          setCurrentSession(null);
          setActiveRewriteUnitId(null);
          setActiveSuggestionId(null);
          setLiveProgress(null);
        });
        showNotice(
          "warning",
          demoRuntime
            ? `已保存并清理，但重新打开失败：${readableError(error)}`
            : `已覆盖并清理，但重新打开失败：${readableError(error)}`
        );
        return;
      }
      try {
        const refreshed = await openDocument(session.documentPath);
        logScrollRestore("finalize-recover-restoring", {
          previousSessionId: session.id,
          reopenedSessionId: refreshed.id,
          preservedScrollTop,
          path: session.documentPath
        });
        await restoreLoadedSessionWithScroll({
          captureDocumentScrollPosition,
          applySessionState,
          session: refreshed,
          preservedScrollTop,
          preferredRewriteUnitId: activeRewriteUnitIdRef.current
        });
        setLiveProgress(null);
      } catch {
        // ignore secondary failure
      }

      showNotice("error", `${actionLabel}失败：${readableError(error)}`);
    }
  }, [
    activeRewriteUnitIdRef,
    applySessionState,
    captureDocumentScrollPosition,
    closeSettings,
    currentSessionRef,
    demoRuntime,
    refreshSessionState,
    requestConfirm,
    setActiveRewriteUnitId,
    setActiveSuggestionId,
    setCurrentSession,
    setLiveProgress,
    showNotice,
    withBusy
  ]);

  const handleResetSession = useCallback(async () => {
    const session = currentSessionRef.current;
    if (!session) {
      showNotice("warning", "当前没有可重置的文档。");
      return;
    }

    if (session.status === "running" || session.status === "paused") {
      showNotice("warning", "文档正在执行自动任务，请先取消后再重置记录。");
      return;
    }

    const stats = getSessionStats(session);
    const hints = demoRuntime
      ? [
        "该操作会删除该文档的全部历史记录（建议、进度），并从网页缓存重新创建会话。",
        "不会覆盖本地文件内容。",
        "",
        `文件：${formatDisplayPath(session.documentPath)}`,
        `当前记录：建议 ${stats.suggestionsTotal}，已应用 ${stats.unitsApplied}/${stats.total}`,
        stats.unitsProposed > 0
          ? `待处理：${stats.unitsProposed}（会一起删除）`
          : "待处理：0",
        stats.pendingGeneration > 0
          ? `未生成：${stats.pendingGeneration}（会一起删除）`
          : "未生成：0"
      ]
      : [
        "该操作会删除该文档的全部历史记录（建议、进度），并从原文件重新创建会话。",
        "不会修改原文件内容。",
        "",
        `文件：${formatDisplayPath(session.documentPath)}`,
        `当前记录：建议 ${stats.suggestionsTotal}，已应用 ${stats.unitsApplied}/${stats.total}`,
        stats.unitsProposed > 0
          ? `待处理：${stats.unitsProposed}（会一起删除）`
          : "待处理：0",
        stats.pendingGeneration > 0
          ? `未生成：${stats.pendingGeneration}（会一起删除）`
          : "未生成：0"
      ];

    const ok = await requestConfirm({
      title: "重置该文档记录",
      message: hints.join("\n"),
      okLabel: "重置记录",
      cancelLabel: "取消",
      variant: "danger"
    });

    if (!ok) return;

    const result = await runSessionActionOrNotify({
      captureDocumentScrollPosition,
      applySessionState,
      showNotice,
      errorPrefix: "重置失败",
      formatError: readableError,
      run: () => withBusy("reset-session", () => resetSession(session.id)),
      resolveState: () => ({
        preferredRewriteUnitId: activeRewriteUnitIdRef.current
      })
    });
    if (!result) {
      return;
    }

    setLiveProgress(null);
    showNotice(
      "success",
      demoRuntime
        ? "已重置记录，并重新从网页缓存创建会话。"
        : "已重置记录，并重新从原文件创建会话。"
    );
  }, [
    activeRewriteUnitIdRef,
    applySessionState,
    captureDocumentScrollPosition,
    currentSessionRef,
    demoRuntime,
    requestConfirm,
    setLiveProgress,
    showNotice,
    withBusy
  ]);

  return { handleExport, handleFinalizeDocument, handleResetSession } as const;
}
