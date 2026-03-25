import { useCallback } from "react";
import {
  cancelRewrite,
  pauseRewrite,
  resumeRewrite,
  retryChunk,
  startRewrite
} from "../../lib/api";
import type {
  ChunkTask,
  DocumentSession,
  RewriteMode,
  RewriteProgress
} from "../../lib/types";
import {
  countCharacters,
  getLatestSuggestion,
  readableError,
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

type RefreshSessionState = (
  sessionId: string,
  options?: {
    preserveChunk?: boolean;
    preferredChunkIndex?: number;
    preserveSuggestion?: boolean;
    preferredSuggestionId?: string | null;
  }
) => Promise<DocumentSession>;

const CHUNK_RISK_WARNING_NON_WHITESPACE_CHARS = 6000;

function findNextManualChunk(session: DocumentSession) {
  return (
    session.chunks.find(
      (chunk) =>
        !chunk.skipRewrite && (chunk.status === "idle" || chunk.status === "failed")
    ) ?? null
  );
}

function findAutoPendingChunks(session: DocumentSession) {
  return session.chunks.filter((chunk) => !chunk.skipRewrite && chunk.status !== "done");
}

function chunkSizeSummary(chunk: ChunkTask) {
  const rawChars = chunk.sourceText.length;
  const nonWhitespaceChars = countCharacters(chunk.sourceText);
  const lineBreaks = chunk.sourceText.split(/\r\n|\r|\n/).length - 1;
  return { rawChars, nonWhitespaceChars, lineBreaks };
}

export function useRewriteActions(options: {
  stageRef: React.MutableRefObject<"workbench" | "editor">;
  currentSessionRef: React.MutableRefObject<DocumentSession | null>;
  activeChunkIndexRef: React.MutableRefObject<number>;
  activeSuggestionIdRef: React.MutableRefObject<string | null>;
  editorDirtyRef: React.MutableRefObject<boolean>;
  requestConfirm: (options: ConfirmModalOptions) => Promise<boolean>;
  applySessionState: ApplySessionState;
  refreshSessionState: RefreshSessionState;
  setReviewView: React.Dispatch<React.SetStateAction<"diff" | "source" | "candidate">>;
  setLiveProgress: React.Dispatch<React.SetStateAction<RewriteProgress | null>>;
  showNotice: ShowNotice;
  withBusy: WithBusy;
}) {
  const {
    stageRef,
    currentSessionRef,
    activeChunkIndexRef,
    activeSuggestionIdRef,
    editorDirtyRef,
    requestConfirm,
    applySessionState,
    refreshSessionState,
    setReviewView,
    setLiveProgress,
    showNotice,
    withBusy
  } = options;

  const confirmIfChunksTooLarge = useCallback(
    async (mode: RewriteMode, session: DocumentSession) => {
      const pending =
        mode === "manual"
          ? [findNextManualChunk(session)].filter(
              (chunk): chunk is ChunkTask => chunk != null
            )
          : findAutoPendingChunks(session);

      if (pending.length === 0) return true;

      const risky = pending
        .map((chunk) => ({
          chunk,
          size: chunkSizeSummary(chunk)
        }))
        .filter(
          ({ size }) => size.nonWhitespaceChars >= CHUNK_RISK_WARNING_NON_WHITESPACE_CHARS
        );

      if (risky.length === 0) return true;

      const maxRisk = risky.reduce((prev, curr) =>
        curr.size.nonWhitespaceChars > prev.size.nonWhitespaceChars ? curr : prev
      );

      const title = "片段过长风险提示";
      const header =
        mode === "manual"
          ? `即将处理的片段过长，可能导致接口报错（上下文超限）或超时。`
          : `待处理队列中存在超长片段，自动批处理可能在中途失败并停止。`;

      const summaryLines =
        mode === "manual"
          ? [
              `目标片段：第 ${maxRisk.chunk.index + 1} 段`,
              `非空字符：${maxRisk.size.nonWhitespaceChars.toLocaleString()}（经验阈值 ${CHUNK_RISK_WARNING_NON_WHITESPACE_CHARS.toLocaleString()}）`,
              `总字符：${maxRisk.size.rawChars.toLocaleString()}`,
              `换行数：${maxRisk.size.lineBreaks.toLocaleString()}`
            ]
          : [
              `待处理片段：${pending.length.toLocaleString()} 段`,
              `超阈值片段：${risky.length.toLocaleString()} 段（经验阈值 ${CHUNK_RISK_WARNING_NON_WHITESPACE_CHARS.toLocaleString()} 非空字符）`,
              `最长片段：第 ${maxRisk.chunk.index + 1} 段 / ${maxRisk.size.nonWhitespaceChars.toLocaleString()} 非空字符`
            ];

      const guidanceLines = [
        "建议操作：",
        "- 返回设置切换为更细粒度（整句/小句）再重试；",
        "- 或先手动把原文拆分为更短段落后再导入；",
        "- 或提高超时/换更大上下文模型。",
        "",
        "系统不会替你自动“降级分块”。选择继续将按当前分块直接调用模型。"
      ];

      const ok = await requestConfirm({
        title,
        message: [header, "", ...summaryLines, "", ...guidanceLines].join("\n"),
        okLabel: "继续优化",
        cancelLabel: "取消并调整",
        variant: "primary"
      });
      return ok;
    },
    [requestConfirm]
  );

  const handleStartRewrite = useCallback(
    async (mode: RewriteMode) => {
      if (stageRef.current === "editor") {
        showNotice(
          "warning",
          editorDirtyRef.current
            ? "你有未保存的手动编辑，请先保存或放弃修改。"
            : "当前处于编辑页，请先返回工作台再执行 AI 优化。"
        );
        return;
      }

      const session = currentSessionRef.current;
      if (!session) {
        showNotice("warning", "请先打开一个文档。");
        return;
      }

      const ok = await confirmIfChunksTooLarge(mode, session);
      if (!ok) {
        showNotice("info", "已取消执行，请调整切段策略或拆分文本后再重试。");
        return;
      }

      try {
        const updated = await withBusy(`start-${mode}`, () => startRewrite(session.id, mode));
        if (mode === "manual") {
          const suggestion = getLatestSuggestion(updated);
          const nextChunkIndex = suggestion?.chunkIndex ?? selectDefaultChunkIndex(updated);

          applySessionState(updated, nextChunkIndex, {
            preferredSuggestionId: suggestion?.id ?? null
          });
          setReviewView("diff");
          showNotice(
            "success",
            suggestion
              ? `已生成修改对 #${suggestion.sequence}，请在右侧审阅。`
              : "已生成下一段，请在右侧审阅。"
          );
          return;
        }

        applySessionState(updated, activeChunkIndexRef.current, {
          preferredSuggestionId: activeSuggestionIdRef.current
        });
        showNotice("info", "自动批处理已启动，系统会后台连续处理并自动应用结果。");
      } catch (error) {
        if (mode === "manual" && session) {
          try {
            await refreshSessionState(session.id, {
              preserveChunk: true,
              preserveSuggestion: true
            });
            setReviewView("diff");
          } catch {
            // 保留原始错误提示，避免二次异常覆盖主错误。
          }
        }
        showNotice("error", `执行失败：${readableError(error)}`);
      }
    },
    [
      activeChunkIndexRef,
      activeSuggestionIdRef,
      applySessionState,
      confirmIfChunksTooLarge,
      currentSessionRef,
      editorDirtyRef,
      refreshSessionState,
      setReviewView,
      showNotice,
      stageRef,
      withBusy
    ]
  );

  const handlePause = useCallback(async () => {
    const session = currentSessionRef.current;
    if (!session) return;
    try {
      const updated = await withBusy("pause-rewrite", () => pauseRewrite(session.id));
      applySessionState(updated, activeChunkIndexRef.current, {
        preferredSuggestionId: activeSuggestionIdRef.current
      });
      showNotice("warning", "自动任务已暂停，可继续或取消。");
    } catch (error) {
      showNotice("error", `暂停失败：${readableError(error)}`);
    }
  }, [
    activeChunkIndexRef,
    activeSuggestionIdRef,
    applySessionState,
    currentSessionRef,
    showNotice,
    withBusy
  ]);

  const handleResume = useCallback(async () => {
    const session = currentSessionRef.current;
    if (!session) return;
    try {
      const updated = await withBusy("resume-rewrite", () => resumeRewrite(session.id));
      applySessionState(updated, activeChunkIndexRef.current, {
        preferredSuggestionId: activeSuggestionIdRef.current
      });
      showNotice("info", "自动任务已继续。");
    } catch (error) {
      showNotice("error", `继续失败：${readableError(error)}`);
    }
  }, [
    activeChunkIndexRef,
    activeSuggestionIdRef,
    applySessionState,
    currentSessionRef,
    showNotice,
    withBusy
  ]);

  const handleCancel = useCallback(async () => {
    const session = currentSessionRef.current;
    if (!session) return;
    try {
      const updated = await withBusy("cancel-rewrite", () => cancelRewrite(session.id));
      applySessionState(updated, activeChunkIndexRef.current, {
        preferredSuggestionId: activeSuggestionIdRef.current
      });
      setLiveProgress(null);
      showNotice("warning", "自动任务已取消，已保留当前文档进度。");
    } catch (error) {
      showNotice("error", `取消失败：${readableError(error)}`);
    }
  }, [
    activeChunkIndexRef,
    activeSuggestionIdRef,
    applySessionState,
    currentSessionRef,
    setLiveProgress,
    showNotice,
    withBusy
  ]);

  const handleRetry = useCallback(async () => {
    const session = currentSessionRef.current;
    const chunk = session?.chunks[activeChunkIndexRef.current];
    if (!session || !chunk) return;
    try {
      const updated = await withBusy("retry-chunk", () => retryChunk(session.id, chunk.index));
      const suggestion = getLatestSuggestion(updated);
      const nextChunkIndex = suggestion?.chunkIndex ?? chunk.index;
      applySessionState(updated, nextChunkIndex, {
        preferredSuggestionId: suggestion?.id ?? null
      });
      setReviewView("diff");
      showNotice(
        "info",
        suggestion
          ? `已重新生成修改对 #${suggestion.sequence}（第 ${chunk.index + 1} 段）。`
          : `第 ${chunk.index + 1} 段已重新生成。`
      );
    } catch (error) {
      try {
        await refreshSessionState(session.id, {
          preferredChunkIndex: chunk.index,
          preserveSuggestion: true
        });
        setReviewView("diff");
      } catch {
        // 保留原始错误提示，避免二次异常覆盖主错误。
      }
      showNotice("error", `重试失败：${readableError(error)}`);
    }
  }, [
    activeChunkIndexRef,
    applySessionState,
    currentSessionRef,
    refreshSessionState,
    setReviewView,
    showNotice,
    withBusy
  ]);

  return { handleStartRewrite, handlePause, handleResume, handleCancel, handleRetry } as const;
}
