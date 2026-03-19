import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  AlertCircle,
  Check,
  Copy,
  FileDiff,
  FileCheck2,
  FolderOpen,
  LoaderCircle,
  Pause,
  Play,
  RotateCcw,
  Settings2,
  Square,
  Trash2,
  WandSparkles,
  X
} from "lucide-react";
import type {
  AppSettings,
  ChunkTask,
  DocumentSession,
  EditSuggestion,
  RewriteMode,
  RewriteProgress,
} from "../lib/types";
import type { SessionStats } from "../lib/helpers";
import type { ReviewView } from "../lib/constants";
import { REVIEW_VIEW_OPTIONS } from "../lib/constants";
import {
  chunkStatusTone,
  countCharacters,
  formatDate,
  formatChunkStatus,
  formatSuggestionDecision,
  getLatestSuggestion,
  groupSuggestionsByChunk,
  isSettingsReady,
  suggestionTone,
  summarizeChunkSuggestions
} from "../lib/helpers";
import { ActionButton } from "../components/ActionButton";
import { Panel } from "../components/Panel";
import { StatusBadge } from "../components/StatusBadge";

type DocumentView = "markup" | "source" | "final";

const DOCUMENT_VIEW_OPTIONS: ReadonlyArray<{
  key: DocumentView;
  label: string;
  hint: string;
}> = [
  { key: "markup", label: "修订标记", hint: "查看插入/删除标记与高亮" },
  { key: "final", label: "修改后", hint: "按当前最新候选合并成整篇" },
  { key: "source", label: "修改前", hint: "查看原文整篇（不含任何改写）" }
];

type CopyState = "idle" | "copying" | "done" | "error";

interface WorkbenchStageProps {
  settings: AppSettings;
  currentSession: DocumentSession | null;
  liveProgress: RewriteProgress | null;
  currentStats: SessionStats | null;
  activeChunk: ChunkTask | null;
  activeChunkIndex: number;
  activeSuggestionId: string | null;
  reviewView: ReviewView;
  busyAction: string | null;
  onOpenDocument: () => void;
  onSelectChunk: (index: number) => void;
  onSelectSuggestion: (suggestionId: string) => void;
  onSetReviewView: (view: ReviewView) => void;
  onStartRewrite: (mode: RewriteMode) => void;
  onPause: () => void;
  onResume: () => void;
  onCancel: () => void;
  onFinalizeDocument: () => void;
  onResetSession: () => void;
  onApplySuggestion: (suggestionId: string) => void;
  onDismissSuggestion: (suggestionId: string) => void;
  onDeleteSuggestion: (suggestionId: string) => void;
  onRetry: () => void;
  onOpenSettings: () => void;
}

export const WorkbenchStage = memo(function WorkbenchStage({
  settings,
  currentSession,
  liveProgress,
  currentStats,
  activeChunk,
  activeChunkIndex,
  activeSuggestionId,
  reviewView,
  busyAction,
  onOpenDocument,
  onSelectChunk,
  onSelectSuggestion,
  onSetReviewView,
  onStartRewrite,
  onPause,
  onResume,
  onCancel,
  onFinalizeDocument,
  onResetSession,
  onApplySuggestion,
  onDismissSuggestion,
  onDeleteSuggestion,
  onRetry,
  onOpenSettings
}: WorkbenchStageProps) {
  const settingsReady = isSettingsReady(settings);
  const [documentView, setDocumentView] = useState<DocumentView>("markup");
  const [copyState, setCopyState] = useState<CopyState>("idle");
  const copyResetTimerRef = useRef<number | null>(null);
  const rewriteRunning = currentSession?.status === "running";
  const rewritePaused = currentSession?.status === "paused";
  const anyBusy = Boolean(busyAction);

  const canStartRewrite = Boolean(
    settingsReady &&
      currentSession &&
      currentStats &&
      !rewriteRunning &&
      !rewritePaused &&
      currentStats.pendingGeneration > 0
  );

  const startKey = `start-${settings.rewriteMode}`;
  const startBusy = busyAction === startKey;
  const pauseBusy = busyAction === "pause-rewrite";
  const resumeBusy = busyAction === "resume-rewrite";
  const cancelBusy = busyAction === "cancel-rewrite";
  const finalizeBusy = busyAction === "finalize-document";
  const hasAppliedEdits = Boolean(currentStats && currentStats.suggestionsApplied > 0);

  const finalizeDisabled =
    finalizeBusy ||
    (anyBusy && busyAction !== "finalize-document") ||
    rewriteRunning ||
    rewritePaused ||
    !hasAppliedEdits;

  const finalizeTitle = useMemo(() => {
    if (finalizeBusy) return "正在写回原文件…";
    if (rewriteRunning || rewritePaused) {
      return "请先取消自动任务后再写回原文件";
    }
    if (!currentStats) return "正在计算会话状态…";
    if (currentStats.suggestionsApplied === 0) {
      return "还没有已应用的修改（先在右侧点“应用”）";
    }
    return "覆盖原文件并清理记录（不可撤销）";
  }, [currentStats, finalizeBusy, rewritePaused, rewriteRunning]);

  const runKey = rewriteRunning
    ? "pause-rewrite"
    : rewritePaused
      ? "resume-rewrite"
      : startKey;
  const runBusy = rewriteRunning ? pauseBusy : rewritePaused ? resumeBusy : startBusy;

  const runLabel = useMemo(() => {
    if (rewriteRunning) return "暂停";
    if (rewritePaused) return "继续";
    return settings.rewriteMode === "auto" ? "开始批处理" : "开始优化";
  }, [rewritePaused, rewriteRunning, settings.rewriteMode]);

  const runTitle = useMemo(() => {
    if (rewriteRunning) return "暂停自动任务";
    if (rewritePaused) return "继续自动任务";
    if (!currentSession) return "请先打开一个文档";
    if (!settingsReady) return "请先在设置里配置 Base URL / Key / Model";
    if (!currentStats) return "正在计算会话状态…";
    if (currentStats.pendingGeneration === 0) {
      return "全部片段已生成，可在右侧审阅并导出";
    }
    return settings.rewriteMode === "auto" ? "自动批处理生成并应用" : "生成下一条修改对";
  }, [
    currentSession,
    currentStats,
    rewritePaused,
    rewriteRunning,
    settings.rewriteMode,
    settingsReady
  ]);

  const documentSubtitle = useMemo(() => {
    if (!currentSession) {
      return "导入文档后可切换：修改前 / 修改后 / 修订标记";
    }
    switch (documentView) {
      case "source":
        return "修改前（原文）";
      case "final":
        return "修改后（合并视图）";
      case "markup":
        return "含修订标记";
      default:
        return "文档";
    }
  }, [currentSession, documentView]);

  useEffect(() => {
    return () => {
      if (copyResetTimerRef.current != null) {
        window.clearTimeout(copyResetTimerRef.current);
        copyResetTimerRef.current = null;
      }
    };
  }, []);

  const suggestionsByChunk = useMemo(
    () => groupSuggestionsByChunk(currentSession?.suggestions ?? []),
    [currentSession?.suggestions]
  );

  const runningIndexSet = useMemo(() => {
    if (!currentSession) return new Set<number>();
    if (!liveProgress) return new Set<number>();
    if (liveProgress.sessionId !== currentSession.id) return new Set<number>();
    return new Set(liveProgress.runningIndices);
  }, [currentSession, liveProgress]);

  const optimisticManualRunningIndex = useMemo(() => {
    if (!currentSession) return null;
    if (busyAction === "retry-chunk") {
      return currentSession.chunks[activeChunkIndex]?.index ?? null;
    }
    if (busyAction !== "start-manual") {
      return null;
    }
    const target = currentSession.chunks.find(
      (chunk) => chunk.status === "idle" || chunk.status === "failed"
    );
    return target?.index ?? null;
  }, [activeChunkIndex, busyAction, currentSession]);

  const copyText = useMemo(() => {
    if (!currentSession) return null;
    if (documentView !== "source" && documentView !== "final") return null;

    return currentSession.chunks
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
  }, [currentSession, documentView, suggestionsByChunk]);

  const copyTitle = useMemo(() => {
    if (documentView === "source") return "复制修改前全文";
    if (documentView === "final") return "复制修改后全文";
    return "复制全文";
  }, [documentView]);

  const writeClipboardText = useCallback(async (text: string) => {
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
  }, []);

  const handleCopyDocument = useCallback(async () => {
    if (!copyText) return;

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
  }, [copyText, writeClipboardText]);

  const activeChunkSuggestions = useMemo(() => {
    if (!currentSession || !activeChunk) return [];
    return suggestionsByChunk.get(activeChunk.index) ?? [];
  }, [currentSession, activeChunk, suggestionsByChunk]);

  const orderedSuggestions = useMemo(() => {
    if (!currentSession) return [];
    return [...currentSession.suggestions].sort((a, b) => a.sequence - b.sequence);
  }, [currentSession]);

  const activeSuggestion = useMemo<EditSuggestion | null>(() => {
    if (!currentSession || !activeSuggestionId) return null;
    return currentSession.suggestions.find((item) => item.id === activeSuggestionId) ?? null;
  }, [currentSession, activeSuggestionId]);

  const latestSuggestion = useMemo(
    () => (currentSession ? getLatestSuggestion(currentSession) : null),
    [currentSession]
  );

  const chunkNodesRef = useRef<Array<HTMLSpanElement | null>>([]);

  useEffect(() => {
    if (!currentSession) return;
    const node = chunkNodesRef.current[activeChunkIndex];
    node?.scrollIntoView({ block: "center", behavior: "smooth" });
  }, [activeChunkIndex, currentSession?.id]);

  const activeCandidateCharacters = activeSuggestion?.afterText
    ? countCharacters(activeSuggestion.afterText)
    : 0;

  return (
    <div className="workbench-root">
      <div className="workbench-layout">
        <div className="workbench-column is-center">
          <Panel
            title="文档"
            subtitle={documentSubtitle}
            bodyClassName="workbench-center-body"
            action={
              currentSession ? (
                <div className="workbench-doc-actionbar">
                  <div className="workbench-doc-actionbar-left" aria-label="文档视图">
                    {DOCUMENT_VIEW_OPTIONS.map((option) => (
                      <button
                        key={option.key}
                        type="button"
                        className={`switch-chip ${
                          documentView === option.key ? "is-active" : ""
                        }`}
                        onClick={() => setDocumentView(option.key)}
                        aria-label={`切换到${option.label}视图`}
                        title={option.hint}
                      >
                        {option.label}
                      </button>
                    ))}
                  </div>

                  <div className="workbench-doc-actionbar-right">
                    {copyText ? (
                      <button
                        type="button"
                        className="icon-button"
                        onClick={() => void handleCopyDocument()}
                        aria-label={copyTitle}
                        title={copyTitle}
                        disabled={copyState === "copying"}
                      >
                        {copyState === "copying" ? (
                          <LoaderCircle className="spin" />
                        ) : copyState === "done" ? (
                          <Check />
                        ) : copyState === "error" ? (
                          <AlertCircle />
                        ) : (
                          <Copy />
                        )}
                      </button>
                    ) : null}

                    <button
                      type="button"
                      className="icon-button"
                      onClick={onResetSession}
                      aria-label="重置该文档记录（不修改原文件）"
                      title="重置该文档记录（不修改原文件）"
                      disabled={
                        !currentSession ||
                        rewriteRunning ||
                        rewritePaused ||
                        busyAction === "reset-session" ||
                        (anyBusy && busyAction !== "reset-session")
                      }
                    >
                      {busyAction === "reset-session" ? (
                        <LoaderCircle className="spin" />
                      ) : (
                        <RotateCcw />
                      )}
                    </button>

                    <button
                      type="button"
                      className={`icon-button ${hasAppliedEdits ? "is-danger" : ""}`}
                      onClick={onFinalizeDocument}
                      aria-label="覆盖原文件并清理记录"
                      title={finalizeTitle}
                      disabled={finalizeDisabled}
                    >
                      {finalizeBusy ? <LoaderCircle className="spin" /> : <FileCheck2 />}
                    </button>

                    <button
                      type="button"
                      className={`toolbar-button ${rewriteRunning ? "is-warning" : "is-primary"}`}
                      onClick={() => {
                        if (rewriteRunning) {
                          onPause();
                          return;
                        }
                        if (rewritePaused) {
                          onResume();
                          return;
                        }
                        onStartRewrite(settings.rewriteMode);
                      }}
                      aria-label={
                        rewriteRunning
                          ? "暂停执行"
                          : rewritePaused
                            ? "继续执行"
                            : settings.rewriteMode === "auto"
                              ? "开始批处理"
                              : "开始优化"
                      }
                      title={runTitle}
                      disabled={
                        rewriteRunning
                          ? pauseBusy || (anyBusy && busyAction !== runKey)
                          : rewritePaused
                            ? resumeBusy || (anyBusy && busyAction !== runKey)
                            : !canStartRewrite || startBusy || (anyBusy && busyAction !== runKey)
                      }
                    >
                      {runBusy ? (
                        <LoaderCircle className="spin" />
                      ) : rewriteRunning ? (
                        <Pause />
                      ) : rewritePaused ? (
                        <Play />
                      ) : (
                        <WandSparkles />
                      )}
                      <span>{runLabel}</span>
                    </button>

                    {rewriteRunning || rewritePaused ? (
                      <button
                        type="button"
                        className="icon-button"
                        onClick={onCancel}
                        aria-label="取消执行"
                        title="取消"
                        disabled={cancelBusy || (anyBusy && busyAction !== "cancel-rewrite")}
                      >
                        {cancelBusy ? <LoaderCircle className="spin" /> : <Square />}
                      </button>
                    ) : null}
                  </div>
                </div>
              ) : null
            }
          >
            {currentSession ? (
              <article className="editor-paper workbench-editor-paper">
                <div className="paper-content scroll-region">
                  <p className="document-flow">
                    {currentSession.chunks.map((chunk) => {
                      const chunkSuggestions =
                        suggestionsByChunk.get(chunk.index) ?? [];
                      const summary = summarizeChunkSuggestions(chunkSuggestions);
                      const displaySuggestion = summary.applied ?? summary.proposed ?? null;

                      const classes = [
                        "doc-chunk",
                        chunk.index === activeChunkIndex ? "is-active" : "",
                        chunk.status === "running" ||
                        runningIndexSet.has(chunk.index) ||
                        chunk.index === optimisticManualRunningIndex
                          ? "is-running"
                          : "",
                        chunk.status === "failed" ? "is-failed" : "",
                        documentView === "markup" && summary.applied ? "is-applied" : "",
                        documentView === "markup" && !summary.applied && summary.proposed
                          ? "is-proposed"
                          : ""
                      ]
                        .filter(Boolean)
                        .join(" ");

                      return (
                        <span key={chunk.index} className="doc-chunk-wrap">
                          <span
                            ref={(node) => {
                              chunkNodesRef.current[chunk.index] = node;
                            }}
                            className={classes}
                            onClick={() => {
                              onSelectChunk(chunk.index);
                              if (displaySuggestion) {
                                onSelectSuggestion(displaySuggestion.id);
                              }
                            }}
                          >
                            {documentView === "source"
                              ? chunk.sourceText
                              : documentView === "final"
                                ? displaySuggestion
                                  ? displaySuggestion.afterText
                                  : chunk.sourceText
                                : displaySuggestion
                                  ? displaySuggestion.diffSpans.map((span, index) => (
                                      <span
                                        key={`${span.type}-${index}-${span.text.length}`}
                                        className={`diff-span is-${span.type}`}
                                      >
                                        {span.text}
                                      </span>
                                    ))
                                  : chunk.sourceText}
                          </span>
                          {chunk.separatorAfter}
                        </span>
                      );
                    })}
                  </p>
                </div>
              </article>
            ) : (
              <div className="empty-state">
                <FolderOpen />
                <div>
                  <strong>打开一个文档开始</strong>
                  <span>
                    LessAI 会为该文件保存优化进度，下次打开同一文件会自动恢复。
                  </span>
                </div>
                <ActionButton
                  icon={FolderOpen}
                  label="打开文件"
                  busy={busyAction === "open-document"}
                  disabled={anyBusy && busyAction !== "open-document"}
                  onClick={onOpenDocument}
                  variant="primary"
                />
                {!settingsReady ? (
                  <ActionButton
                    icon={Settings2}
                    label="先去设置接口与模型"
                    busy={false}
                    onClick={onOpenSettings}
                    variant="secondary"
                  />
                ) : null}
              </div>
            )}
          </Panel>
        </div>

        <div className="workbench-column is-right">
          <Panel
            title="审阅"
            subtitle="修改对时间线（可追溯 / 可应用 / 可撤销）"
            bodyClassName="workbench-review-body"
            action={
              <div className="workbench-review-actionbar">
                {currentSession && activeSuggestion ? (
                  <StatusBadge tone={suggestionTone(activeSuggestion.decision)}>
                    #{activeSuggestion.sequence}{" "}
                    {formatSuggestionDecision(activeSuggestion.decision)}
                  </StatusBadge>
                ) : currentSession && activeChunk ? (
                  <StatusBadge
                    tone={chunkStatusTone(activeChunk, activeChunkSuggestions)}
                  >
                    {formatChunkStatus(activeChunk, activeChunkSuggestions)}
                  </StatusBadge>
                ) : (
                  <StatusBadge tone={settingsReady ? "info" : "warning"}>
                    {settingsReady ? "等待生成" : "未配置"}
                  </StatusBadge>
                )}

                <div className="workbench-review-actionbar-buttons">
                  {currentSession && activeChunk?.status === "failed" ? (
                    <button
                      type="button"
                      className="icon-button icon-button-sm"
                      onClick={onRetry}
                      aria-label="重试生成当前位置"
                      title="重试生成当前位置"
                      disabled={
                        !settingsReady ||
                        rewriteRunning ||
                        rewritePaused ||
                        busyAction === "retry-chunk" ||
                        (anyBusy && busyAction !== "retry-chunk")
                      }
                    >
                      {busyAction === "retry-chunk" ? (
                        <LoaderCircle className="spin" />
                      ) : (
                        <RotateCcw />
                      )}
                    </button>
                  ) : null}

                  {activeSuggestion ? (
                    <>
                      <button
                        type="button"
                        className="icon-button icon-button-sm"
                        onClick={() => onApplySuggestion(activeSuggestion.id)}
                        aria-label="应用该修改对"
                        title="应用"
                        disabled={
                          rewriteRunning ||
                          rewritePaused ||
                          activeSuggestion.decision === "applied" ||
                          busyAction === `apply-suggestion:${activeSuggestion.id}` ||
                          (anyBusy &&
                            busyAction !== `apply-suggestion:${activeSuggestion.id}`)
                        }
                      >
                        {busyAction === `apply-suggestion:${activeSuggestion.id}` ? (
                          <LoaderCircle className="spin" />
                        ) : (
                          <Check />
                        )}
                      </button>
                      <button
                        type="button"
                        className="icon-button icon-button-sm"
                        onClick={() => onDismissSuggestion(activeSuggestion.id)}
                        aria-label={
                          activeSuggestion.decision === "applied"
                            ? "取消应用该修改对"
                            : "忽略该修改对"
                        }
                        title={
                          activeSuggestion.decision === "applied"
                            ? "取消应用"
                            : "忽略"
                        }
                        disabled={
                          rewriteRunning ||
                          rewritePaused ||
                          activeSuggestion.decision === "dismissed" ||
                          busyAction === `dismiss-suggestion:${activeSuggestion.id}` ||
                          (anyBusy &&
                            busyAction !== `dismiss-suggestion:${activeSuggestion.id}`)
                        }
                      >
                        {busyAction === `dismiss-suggestion:${activeSuggestion.id}` ? (
                          <LoaderCircle className="spin" />
                        ) : activeSuggestion.decision === "applied" ? (
                          <RotateCcw />
                        ) : (
                          <X />
                        )}
                      </button>
                      <button
                        type="button"
                        className="icon-button icon-button-sm"
                        onClick={() => onDeleteSuggestion(activeSuggestion.id)}
                        aria-label="删除该修改对"
                        title="删除"
                        disabled={
                          rewriteRunning ||
                          rewritePaused ||
                          busyAction === `delete-suggestion:${activeSuggestion.id}` ||
                          (anyBusy &&
                            busyAction !== `delete-suggestion:${activeSuggestion.id}`)
                        }
                      >
                        {busyAction === `delete-suggestion:${activeSuggestion.id}` ? (
                          <LoaderCircle className="spin" />
                        ) : (
                          <Trash2 />
                        )}
                      </button>
                    </>
                  ) : null}
                </div>
              </div>
            }
          >
            {currentSession && currentStats ? (
              <>
                <div className="context-group">
                  <span className="context-chip">
                    修改对：{currentStats.suggestionsTotal}
                  </span>
                  <span className="context-chip">
                    待审阅：{currentStats.suggestionsProposed}
                  </span>
                  <span className="context-chip">
                    已应用：{currentStats.chunksApplied}/{currentStats.total}
                  </span>
                  <span className="context-chip">
                    候选稿：{activeCandidateCharacters} 字
                  </span>
                  <span className="context-chip">
                    {activeSuggestion
                      ? `当前 #${activeSuggestion.sequence}`
                      : latestSuggestion
                        ? `最新 #${latestSuggestion.sequence}`
                        : "暂无修改对"}
                  </span>
                </div>

                {activeSuggestion ? (
                  <div className="review-switches">
                    {REVIEW_VIEW_OPTIONS.map((item) => (
                      <button
                        key={item.key}
                        type="button"
                        className={`switch-chip ${
                          reviewView === item.key ? "is-active" : ""
                        }`}
                        onClick={() => onSetReviewView(item.key)}
                      >
                        {item.label}
                      </button>
                    ))}
                  </div>
                ) : null}

                {activeChunk?.status === "failed" ? (
                  <div className="error-card">
                    <AlertCircle />
                    <div>
                      <strong>该片段生成失败</strong>
                      <span>
                        {activeChunk.errorMessage ?? "请点击重试重新生成。"}
                      </span>
                    </div>
                  </div>
                ) : null}

                {activeSuggestion ? (
                  <div className="diff-view">
                    {reviewView === "diff" ? (
                      activeSuggestion.diffSpans.length > 0 ? (
                        <p>
                          {activeSuggestion.diffSpans.map((span, index) => (
                            <span
                              key={`${span.type}-${index}-${span.text.length}`}
                              className={`diff-span is-${span.type}`}
                            >
                              {span.text}
                            </span>
                          ))}
                        </p>
                      ) : (
                        <div className="empty-inline">
                          <span>该修改对没有可展示的 diff。</span>
                        </div>
                      )
                    ) : null}

                    {reviewView === "source" ? (
                      <p>{activeSuggestion.beforeText}</p>
                    ) : null}

                    {reviewView === "candidate" ? (
                      <p>{activeSuggestion.afterText}</p>
                    ) : null}
                  </div>
                ) : (
                  <div className="empty-inline">
                    <span>点击下方任意修改对查看细节。</span>
                  </div>
                )}

                <div className="suggestion-list scroll-region">
                  {orderedSuggestions.length === 0 ? (
                    <div className="empty-inline">
                      <span>
                        还没有修改对。点击左侧「文档」右上角的“开始优化”生成一段。
                      </span>
                    </div>
                  ) : (
                    orderedSuggestions.map((suggestion) => (
                      <button
                        key={suggestion.id}
                        type="button"
                        className={`suggestion-row ${
                          suggestion.id === activeSuggestionId ? "is-active" : ""
                        }`}
                        onClick={() => {
                          onSelectChunk(suggestion.chunkIndex);
                          onSelectSuggestion(suggestion.id);
                        }}
                      >
                        <div className="suggestion-row-head">
                          <strong>
                            #{suggestion.sequence} ·{" "}
                            {suggestion.beforeText
                              .trim()
                              .replace(/\s+/g, " ")
                              .slice(0, 24) || "（空片段）"}
                            {suggestion.beforeText.trim().length > 24 ? "…" : ""}
                          </strong>
                          <StatusBadge tone={suggestionTone(suggestion.decision)}>
                            {formatSuggestionDecision(suggestion.decision)}
                          </StatusBadge>
                        </div>
                        <div className="suggestion-row-meta">
                          <span>{formatDate(suggestion.createdAt)}</span>
                          <span>{countCharacters(suggestion.afterText)} 字</span>
                        </div>
                        <p className="suggestion-row-preview">{suggestion.afterText}</p>
                      </button>
                    ))
                  )}
                </div>
              </>
            ) : (
                <div className="empty-state">
                  <FileDiff />
                  <div>
                    <strong>审阅区会展示 diff 与候选稿</strong>
                    <span>先打开一个文档，然后点击左侧文档右上角的“开始优化”。</span>
                  </div>
                <ActionButton
                  icon={FolderOpen}
                  label="打开文件"
                  busy={busyAction === "open-document"}
                  disabled={anyBusy && busyAction !== "open-document"}
                  onClick={onOpenDocument}
                  variant="secondary"
                />
                {!settingsReady ? (
                  <ActionButton
                    icon={Settings2}
                    label="打开设置"
                    busy={false}
                    onClick={onOpenSettings}
                    variant="primary"
                  />
                ) : null}
              </div>
            )}
          </Panel>
        </div>
      </div>
    </div>
  );
});
