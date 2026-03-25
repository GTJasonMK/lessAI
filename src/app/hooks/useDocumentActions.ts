import { open } from "@tauri-apps/plugin-dialog";
import { startTransition, useCallback } from "react";
import { openDocument, saveDocumentEdits } from "../../lib/api";
import type { DocumentSession, RewriteProgress } from "../../lib/types";
import {
  isDocxPath,
  normalizeNewlines,
  readableError,
  selectDefaultChunkIndex
} from "../../lib/helpers";
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

export function useDocumentActions(options: {
  busyAction: string | null;
  stageRef: React.MutableRefObject<"workbench" | "editor">;
  currentSessionRef: React.MutableRefObject<DocumentSession | null>;
  editorDirtyRef: React.MutableRefObject<boolean>;
  editorTextRef: React.MutableRefObject<string>;
  editorBaselineTextRef: React.MutableRefObject<string>;
  applySessionState: ApplySessionState;
  setStage: React.Dispatch<React.SetStateAction<"workbench" | "editor">>;
  setReviewView: React.Dispatch<React.SetStateAction<"diff" | "source" | "candidate">>;
  setEditorBaselineText: React.Dispatch<React.SetStateAction<string>>;
  setEditorText: React.Dispatch<React.SetStateAction<string>>;
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
    editorDirtyRef,
    editorTextRef,
    editorBaselineTextRef,
    applySessionState,
    setStage,
    setReviewView,
    setEditorBaselineText,
    setEditorText,
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
        filters: [{ name: "Documents", extensions: ["txt", "md", "tex", "docx"] }]
      });
      if (!selection) return;

      const path = Array.isArray(selection) ? selection[0] : selection;
      if (!path) return;

      const opened = await withBusy("open-document", () => openDocument(path));
      applySessionState(opened, selectDefaultChunkIndex(opened));
      setReviewView("diff");
      setStage("workbench");
      setEditorBaselineText("");
      setEditorText("");
      closeSettings();
      showNotice(
        "success",
        `已打开文档：${opened.title}（共 ${opened.chunks.length} 段，可继续上次进度）。`
      );
    } catch (error) {
      showNotice("error", `打开失败：${readableError(error)}`);
    }
  }, [
    applySessionState,
    closeSettings,
    currentSessionRef,
    editorDirtyRef,
    setEditorBaselineText,
    setEditorText,
    setReviewView,
    setStage,
    showNotice,
    stageRef,
    withBusy
  ]);

  const handleEnterEditor = useCallback(() => {
    const session = currentSessionRef.current;
    if (!session) {
      showNotice("warning", "请先打开一个文档。");
      return;
    }

    if (isDocxPath(session.documentPath)) {
      showNotice(
        "warning",
        "docx 目前仅支持导入/改写/导出，暂不支持终稿编辑或写回覆盖。"
      );
      return;
    }

    if (busyAction) {
      showNotice("warning", "当前有操作在执行，请稍后再试。");
      return;
    }

    if (session.status === "running" || session.status === "paused") {
      showNotice("warning", "文档正在执行自动任务，请先取消后再编辑。");
      return;
    }

    const cleanSession =
      session.status === "idle" &&
      session.suggestions.length === 0 &&
      session.chunks.every((chunk) => chunk.status === "idle" || chunk.skipRewrite);

    if (!cleanSession) {
      showNotice(
        "warning",
        "该文档存在修订记录或进度，为避免冲突，请先“覆写并清理记录”或“重置记录”后再编辑。"
      );
      return;
    }

    startTransition(() => {
      setStage("editor");
      const normalized = normalizeNewlines(session.sourceText);
      setEditorBaselineText(normalized);
      setEditorText(normalized);
      setLiveProgress(null);
      setSettingsOpen(false);
    });
  }, [
    busyAction,
    currentSessionRef,
    setEditorBaselineText,
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
      setEditorText(editorBaselineTextRef.current);
    });
    showNotice("warning", "已放弃未保存的修改。");
  }, [editorBaselineTextRef, editorDirtyRef, setEditorText, showNotice, stageRef]);

  const handleExitEditor = useCallback(() => {
    if (stageRef.current !== "editor") return;
    if (editorDirtyRef.current) {
      showNotice("warning", "你有未保存的手动编辑，请先保存或放弃修改。");
      return;
    }
    setStage("workbench");
  }, [editorDirtyRef, setStage, showNotice, stageRef]);

  const handleSaveEditor = useCallback(
    async (options?: { returnToWorkbench?: boolean }) => {
      const session = currentSessionRef.current;
      if (!session) return;
      if (stageRef.current !== "editor") return;

      if (!editorDirtyRef.current) {
        showNotice("info", "没有修改，无需保存。");
        if (options?.returnToWorkbench) {
          setStage("workbench");
        }
        return;
      }

      const returnToWorkbench = Boolean(options?.returnToWorkbench);
      const actionKey = returnToWorkbench ? "save-edits-and-back" : "save-edits";
      const content = editorTextRef.current;

      try {
        const updated = await withBusy(actionKey, () => saveDocumentEdits(session.id, content));

        applySessionState(updated, selectDefaultChunkIndex(updated));
        setReviewView("diff");
        setLiveProgress(null);

        startTransition(() => {
          const normalized = normalizeNewlines(updated.sourceText);
          setEditorBaselineText(normalized);
          setEditorText(normalized);
        });

        if (returnToWorkbench) {
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
      applySessionState,
      currentSessionRef,
      editorDirtyRef,
      editorTextRef,
      setEditorBaselineText,
      setEditorText,
      setLiveProgress,
      setReviewView,
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
