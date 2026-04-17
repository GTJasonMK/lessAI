import { useCallback } from "react";
import {
  cancelRewrite,
  pauseRewrite,
  resumeRewrite,
  retryRewriteUnit,
  startRewrite
} from "../../lib/api";
import type {
  DocumentSession,
  RewriteMode,
  RewriteProgress,
  RewriteUnit
} from "../../lib/types";
import {
  countCharacters,
  findRewriteUnit,
  getLatestSuggestion,
  readableError,
  rewriteUnitSourceText
} from "../../lib/helpers";
import {
  findAutoPendingTargetRewriteUnits,
  findNextManualTargetRewriteUnit,
  hasSelectedRewriteUnits,
  normalizeSelectedRewriteUnitIds
} from "../../lib/rewriteUnitSelection";
import type { ConfirmModalOptions } from "../../components/ConfirmModal";
import {
  refreshSessionStateSilently,
  refreshRewriteableSessionOrNotify,
  runSessionActionOrNotify,
  type ApplySessionState,
  type RefreshSessionState,
  type ShowNotice,
  type WithBusy
} from "./sessionActionShared";

const REWRITE_UNIT_RISK_WARNING_NON_WHITESPACE_CHARS = 6000;

function rewriteUnitSizeSummary(session: DocumentSession, rewriteUnit: RewriteUnit) {
  const sourceText = rewriteUnitSourceText(session, rewriteUnit);
  const rawChars = sourceText.length;
  const nonWhitespaceChars = countCharacters(sourceText);
  const lineBreaks = sourceText.split(/\r\n|\r|\n/).length - 1;
  return { rawChars, nonWhitespaceChars, lineBreaks };
}

export function useRewriteActions(options: {
  stageRef: React.MutableRefObject<"workbench" | "editor">;
  currentSessionRef: React.MutableRefObject<DocumentSession | null>;
  activeRewriteUnitIdRef: React.MutableRefObject<string | null>;
  activeSuggestionIdRef: React.MutableRefObject<string | null>;
  selectedRewriteUnitIdsRef: React.MutableRefObject<string[]>;
  captureDocumentScrollPosition: () => number | null;
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
    activeRewriteUnitIdRef,
    activeSuggestionIdRef,
    selectedRewriteUnitIdsRef,
    captureDocumentScrollPosition,
    editorDirtyRef,
    requestConfirm,
    applySessionState,
    refreshSessionState,
    setReviewView,
    setLiveProgress,
    showNotice,
    withBusy
  } = options;

  const confirmIfRewriteUnitsTooLarge = useCallback(
    async (
      mode: RewriteMode,
      session: DocumentSession,
      selectedRewriteUnitIds: readonly string[]
    ) => {
      const pending =
        mode === "manual"
          ? [findNextManualTargetRewriteUnit(session, selectedRewriteUnitIds)].filter(
              (rewriteUnit): rewriteUnit is RewriteUnit => rewriteUnit != null
            )
          : findAutoPendingTargetRewriteUnits(session, selectedRewriteUnitIds);

      if (pending.length === 0) return true;

      const risky = pending
        .map((rewriteUnit) => ({
          rewriteUnit,
          size: rewriteUnitSizeSummary(session, rewriteUnit)
        }))
        .filter(
          ({ size }) => size.nonWhitespaceChars >= REWRITE_UNIT_RISK_WARNING_NON_WHITESPACE_CHARS
        );

      if (risky.length === 0) return true;

      const maxRisk = risky.reduce((prev, curr) =>
        curr.size.nonWhitespaceChars > prev.size.nonWhitespaceChars ? curr : prev
      );

      const title = "片段过长风险提示";
      const header =
        mode === "manual"
          ? "即将处理的片段过长，可能导致接口报错（上下文超限）或超时。"
          : "待处理队列中存在超长片段，自动批处理可能在中途失败并停止。";

      const summaryLines =
        mode === "manual"
          ? [
              `目标片段：第 ${maxRisk.rewriteUnit.order + 1} 段`,
              `非空字符：${maxRisk.size.nonWhitespaceChars.toLocaleString()}（经验阈值 ${REWRITE_UNIT_RISK_WARNING_NON_WHITESPACE_CHARS.toLocaleString()}）`,
              `总字符：${maxRisk.size.rawChars.toLocaleString()}`,
              `换行数：${maxRisk.size.lineBreaks.toLocaleString()}`
            ]
          : [
              `待处理片段：${pending.length.toLocaleString()} 段`,
              `超阈值片段：${risky.length.toLocaleString()} 段（经验阈值 ${REWRITE_UNIT_RISK_WARNING_NON_WHITESPACE_CHARS.toLocaleString()} 非空字符）`,
              `最长片段：第 ${maxRisk.rewriteUnit.order + 1} 段 / ${maxRisk.size.nonWhitespaceChars.toLocaleString()} 非空字符`
            ];

      const guidanceLines = [
        "建议操作：",
        "- 返回设置切换为更细粒度（整句/小句）再重试；",
        "- 或先手动把原文拆分为更短段落后再导入；",
        "- 或提高超时/换更大上下文模型。",
        "",
        "系统不会替你自动“降级分块”。选择继续将按当前分块直接调用模型。"
      ];

      return requestConfirm({
        title,
        message: [header, "", ...summaryLines, "", ...guidanceLines].join("\n"),
        okLabel: "继续优化",
        cancelLabel: "取消并调整",
        variant: "primary"
      });
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
      const latestSession = await refreshRewriteableSessionOrNotify({
        session,
        refreshSessionState,
        options: {
          preserveRewriteUnit: true,
          preserveSuggestion: true
        },
        showNotice,
        errorPrefix: "执行失败",
        formatError: readableError
      });
      if (!latestSession) {
        return;
      }

      const normalizedSelectedRewriteUnitIds = normalizeSelectedRewriteUnitIds(
        latestSession,
        selectedRewriteUnitIdsRef.current
      );
      const targetRewriteUnitIds = hasSelectedRewriteUnits(normalizedSelectedRewriteUnitIds)
        ? normalizedSelectedRewriteUnitIds
        : undefined;

      const ok = await confirmIfRewriteUnitsTooLarge(
        mode,
        latestSession,
        normalizedSelectedRewriteUnitIds
      );
      if (!ok) {
        showNotice("info", "已取消执行，请调整切段策略或拆分文本后再重试。");
        return;
      }

      const result = await runSessionActionOrNotify({
        captureDocumentScrollPosition,
        applySessionState,
        showNotice,
        errorPrefix: "执行失败",
        formatError: readableError,
        run: () =>
          withBusy(`start-${mode}`, () =>
            startRewrite(latestSession.id, mode, targetRewriteUnitIds)
          ),
        resolveState: (updatedSession) => {
          if (mode === "manual") {
            const existingSuggestionIds = new Set(latestSession.suggestions.map((item) => item.id));
            const newSuggestions = updatedSession.suggestions
              .filter((item) => !existingSuggestionIds.has(item.id))
              .sort((left, right) => left.sequence - right.sequence);
            const preferredSuggestion =
              newSuggestions[0] ?? getLatestSuggestion(updatedSession) ?? null;
            return {
              preferredRewriteUnitId: preferredSuggestion?.rewriteUnitId,
              preferredSuggestionId: preferredSuggestion?.id ?? null
            };
          }

          return {
            preferredRewriteUnitId: activeRewriteUnitIdRef.current,
            preferredSuggestionId: activeSuggestionIdRef.current
          };
        },
        recover: async () => {
          if (mode === "manual") {
            setReviewView("diff");
          }
          await refreshSessionStateSilently({
            sessionId: session.id,
            refreshSessionState,
            options: {
              preserveRewriteUnit: true,
              preserveSuggestion: true
            }
          });
        }
      });
      if (!result) {
        return;
      }

      if (mode === "manual") {
        const existingSuggestionIds = new Set(latestSession.suggestions.map((item) => item.id));
        const newSuggestions = result.session.suggestions
          .filter((item) => !existingSuggestionIds.has(item.id))
          .sort((left, right) => left.sequence - right.sequence);
        const preferredSuggestion = newSuggestions[0] ?? getLatestSuggestion(result.session) ?? null;
        setReviewView("diff");
        showNotice(
          "success",
          newSuggestions.length > 1
            ? `已生成 ${newSuggestions.length} 条修改对，请在右侧审阅。`
            : preferredSuggestion
              ? `已生成修改对 #${preferredSuggestion.sequence}，请在右侧审阅。`
              : "已生成下一段，请在右侧审阅。"
        );
        return;
      }

      showNotice("info", "自动批处理已启动，系统会后台连续处理并自动应用结果。");
    },
    [
      activeRewriteUnitIdRef,
      activeSuggestionIdRef,
      applySessionState,
      captureDocumentScrollPosition,
      confirmIfRewriteUnitsTooLarge,
      currentSessionRef,
      editorDirtyRef,
      refreshSessionState,
      selectedRewriteUnitIdsRef,
      setReviewView,
      showNotice,
      stageRef,
      withBusy
    ]
  );

  const handlePause = useCallback(async () => {
    const session = currentSessionRef.current;
    if (!session) return;
    const result = await runSessionActionOrNotify({
      captureDocumentScrollPosition,
      applySessionState,
      showNotice,
      errorPrefix: "暂停失败",
      formatError: readableError,
      run: () => withBusy("pause-rewrite", () => pauseRewrite(session.id)),
      resolveState: () => ({
        preferredRewriteUnitId: activeRewriteUnitIdRef.current,
        preferredSuggestionId: activeSuggestionIdRef.current
      })
    });
    if (!result) {
      return;
    }

    showNotice("warning", "自动任务已暂停，可继续或取消。");
  }, [
    activeRewriteUnitIdRef,
    activeSuggestionIdRef,
    applySessionState,
    captureDocumentScrollPosition,
    currentSessionRef,
    showNotice,
    withBusy
  ]);

  const handleResume = useCallback(async () => {
    const session = currentSessionRef.current;
    if (!session) return;
    const result = await runSessionActionOrNotify({
      captureDocumentScrollPosition,
      applySessionState,
      showNotice,
      errorPrefix: "继续失败",
      formatError: readableError,
      run: () => withBusy("resume-rewrite", () => resumeRewrite(session.id)),
      resolveState: () => ({
        preferredRewriteUnitId: activeRewriteUnitIdRef.current,
        preferredSuggestionId: activeSuggestionIdRef.current
      })
    });
    if (!result) {
      return;
    }

    showNotice("info", "自动任务已继续。");
  }, [
    activeRewriteUnitIdRef,
    activeSuggestionIdRef,
    applySessionState,
    captureDocumentScrollPosition,
    currentSessionRef,
    showNotice,
    withBusy
  ]);

  const handleCancel = useCallback(async () => {
    const session = currentSessionRef.current;
    if (!session) return;
    const result = await runSessionActionOrNotify({
      captureDocumentScrollPosition,
      applySessionState,
      showNotice,
      errorPrefix: "取消失败",
      formatError: readableError,
      run: () => withBusy("cancel-rewrite", () => cancelRewrite(session.id)),
      resolveState: () => ({
        preferredRewriteUnitId: activeRewriteUnitIdRef.current,
        preferredSuggestionId: activeSuggestionIdRef.current
      })
    });
    if (!result) {
      return;
    }

    setLiveProgress(null);
    showNotice("warning", "自动任务已取消，已保留当前文档进度。");
  }, [
    activeRewriteUnitIdRef,
    activeSuggestionIdRef,
    applySessionState,
    captureDocumentScrollPosition,
    currentSessionRef,
    setLiveProgress,
    showNotice,
    withBusy
  ]);

  const handleRetry = useCallback(async () => {
    const session = currentSessionRef.current;
    const rewriteUnit = findRewriteUnit(session ?? null as never, activeRewriteUnitIdRef.current);
    if (!session || !rewriteUnit) return;
    const latestSession = await refreshRewriteableSessionOrNotify({
      session,
      refreshSessionState,
      options: {
        preferredRewriteUnitId: rewriteUnit.id,
        preserveSuggestion: true
      },
      showNotice,
      errorPrefix: "重试失败",
      formatError: readableError
    });
    if (!latestSession) {
      return;
    }
    const latestRewriteUnit = findRewriteUnit(latestSession, rewriteUnit.id);
    if (!latestRewriteUnit) {
      showNotice("warning", "当前片段已不存在，请刷新后重试。");
      return;
    }
    const result = await runSessionActionOrNotify({
      captureDocumentScrollPosition,
      applySessionState,
      showNotice,
      errorPrefix: "重试失败",
      formatError: readableError,
      run: () =>
        withBusy("retry-rewrite-unit", () =>
          retryRewriteUnit(latestSession.id, latestRewriteUnit.id)
        ),
      resolveState: (updatedSession) => {
        const suggestion = getLatestSuggestion(updatedSession);
        return {
          preferredRewriteUnitId: suggestion?.rewriteUnitId ?? latestRewriteUnit.id,
          preferredSuggestionId: suggestion?.id ?? null
        };
      },
      recover: async () => {
        await refreshSessionStateSilently({
          sessionId: latestSession.id,
          refreshSessionState,
          options: {
            preferredRewriteUnitId: latestRewriteUnit.id,
            preserveSuggestion: true
          },
          afterRefresh: async () => {
            setReviewView("diff");
          }
        });
      }
    });
    if (!result) {
      return;
    }

    const suggestion = getLatestSuggestion(result.session);
    setReviewView("diff");
    showNotice(
      "info",
      suggestion
        ? `已重新生成修改对 #${suggestion.sequence}（第 ${latestRewriteUnit.order + 1} 段）。`
        : `第 ${latestRewriteUnit.order + 1} 段已重新生成。`
    );
  }, [
    activeRewriteUnitIdRef,
    applySessionState,
    captureDocumentScrollPosition,
    currentSessionRef,
    refreshSessionState,
    setReviewView,
    showNotice,
    withBusy
  ]);

  return { handleStartRewrite, handlePause, handleResume, handleCancel, handleRetry } as const;
}
