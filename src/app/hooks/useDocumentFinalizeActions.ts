import { save } from "@tauri-apps/plugin-dialog";
import { startTransition, useCallback } from "react";
import {
  exportDocument,
  finalizeDocument,
  openDocument,
  resetSession
} from "../../lib/api";
import type { DocumentSession, RewriteProgress } from "../../lib/types";
import {
  formatDisplayPath,
  getSessionStats,
  isDocxPath,
  readableError,
  sanitizeFileName,
  selectDefaultChunkIndex
} from "../../lib/helpers";
import type { ConfirmModalOptions } from "../../components/ConfirmModal";
import type { NoticeTone } from "../../lib/constants";

type ShowNotice = (
  tone: NoticeTone,
  message: string,
  options?: { autoDismissMs?: number | null }
) => void;

type WithBusy = <T>(action: string, fn: () => Promise<T>) => Promise<T>;

type ApplySessionState = (
  session: DocumentSession,
  nextChunkIndex: number,
  options?: { preferredSuggestionId?: string | null }
) => void;

export function useDocumentFinalizeActions(options: {
  stageRef: React.MutableRefObject<"workbench" | "editor">;
  currentSessionRef: React.MutableRefObject<DocumentSession | null>;
  editorDirtyRef: React.MutableRefObject<boolean>;
  requestConfirm: (options: ConfirmModalOptions) => Promise<boolean>;
  applySessionState: ApplySessionState;
  setCurrentSession: React.Dispatch<React.SetStateAction<DocumentSession | null>>;
  setActiveChunkIndex: React.Dispatch<React.SetStateAction<number>>;
  setActiveSuggestionId: React.Dispatch<React.SetStateAction<string | null>>;
  setReviewView: React.Dispatch<React.SetStateAction<"diff" | "source" | "candidate">>;
  setLiveProgress: React.Dispatch<React.SetStateAction<RewriteProgress | null>>;
  closeSettings: () => void;
  showNotice: ShowNotice;
  withBusy: WithBusy;
}) {
  const {
    stageRef,
    currentSessionRef,
    editorDirtyRef,
    requestConfirm,
    applySessionState,
    setCurrentSession,
    setActiveChunkIndex,
    setActiveSuggestionId,
    setReviewView,
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
      const path = await save({
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
      showNotice("warning", "当前没有可写回的文档。");
      return;
    }

    if (isDocxPath(session.documentPath)) {
      showNotice(
        "warning",
        "docx 暂不支持写回覆盖（会破坏文件结构）。请先“导出”为纯文本后再写回。"
      );
      return;
    }

    if (session.status === "running" || session.status === "paused") {
      showNotice("warning", "文档正在执行自动任务，请先取消后再写回原文件。");
      return;
    }

    const stats = getSessionStats(session);
    const hints = [
      "该操作会把【已应用】的修改覆盖写回原文件，并删除该文档的全部历史记录（修改对、进度）。",
      "不可撤销，建议你先“导出”做一份备份。",
      "写回成功后会自动重新打开该文件（以全新会话展示）。",
      "",
      `文件：${formatDisplayPath(session.documentPath)}`,
      `已应用：${stats.chunksApplied}/${stats.total}`,
      stats.suggestionsProposed > 0
        ? `注意：仍有 ${stats.suggestionsProposed} 条待审阅修改对，不会写入文件。`
        : "待审阅：0（将完整写回已应用结果）",
      stats.pendingGeneration > 0
        ? `注意：仍有 ${stats.pendingGeneration} 段未生成/失败，写回时会保留原文。`
        : "未生成：0"
    ];

    const ok = await requestConfirm({
      title: "覆盖原文件并清理记录",
      message: hints.join("\n"),
      okLabel: "覆盖并清理",
      cancelLabel: "取消",
      variant: "danger"
    });

    if (!ok) return;

    let savedPath: string | null = null;
    try {
      const reopened = await withBusy("finalize-document", async () => {
        savedPath = await finalizeDocument(session.id);
        return openDocument(savedPath);
      });

      applySessionState(reopened, selectDefaultChunkIndex(reopened));
      setReviewView("diff");
      setLiveProgress(null);
      closeSettings();
      showNotice("success", `已覆盖并清理，并重新打开：${savedPath ? formatDisplayPath(savedPath) : ""}`);
    } catch (error) {
      if (savedPath) {
        startTransition(() => {
          setCurrentSession(null);
          setActiveChunkIndex(0);
          setActiveSuggestionId(null);
          setReviewView("diff");
          setLiveProgress(null);
        });
        showNotice("warning", `已覆盖并清理，但重新打开失败：${readableError(error)}`);
        return;
      }

      showNotice("error", `写回失败：${readableError(error)}`);
    }
  }, [
    applySessionState,
    closeSettings,
    currentSessionRef,
    requestConfirm,
    setActiveChunkIndex,
    setActiveSuggestionId,
    setCurrentSession,
    setLiveProgress,
    setReviewView,
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
    const hints = [
      "该操作会删除该文档的全部历史记录（修改对、进度），并从原文件重新创建会话。",
      "不会修改原文件内容。",
      "",
      `文件：${formatDisplayPath(session.documentPath)}`,
      `当前记录：修改对 ${stats.suggestionsTotal}，已应用 ${stats.chunksApplied}/${stats.total}`,
      stats.suggestionsProposed > 0
        ? `待审阅：${stats.suggestionsProposed}（会一起删除）`
        : "待审阅：0",
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

    try {
      const rebuilt = await withBusy("reset-session", () => resetSession(session.id));
      applySessionState(rebuilt, selectDefaultChunkIndex(rebuilt));
      setReviewView("diff");
      setLiveProgress(null);
      showNotice("success", "已重置记录，并重新从原文件创建会话。");
    } catch (error) {
      showNotice("error", `重置失败：${readableError(error)}`);
    }
  }, [applySessionState, currentSessionRef, requestConfirm, setLiveProgress, setReviewView, showNotice, withBusy]);

  return { handleExport, handleFinalizeDocument, handleResetSession } as const;
}
