import {
  memo,
  useCallback,
  useDeferredValue,
  useEffect,
  useMemo,
  useRef,
  useState
} from "react";
import type { ClipboardEvent } from "react";
import {
  AlertCircle,
  ArrowLeft,
  Check,
  Copy,
  FileDiff,
  FileCheck2,
  FilePenLine,
  FolderOpen,
  LoaderCircle,
  Pause,
  Play,
  RotateCcw,
  Save,
  Settings2,
  Square,
  Trash2,
  Undo2,
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
  isDocxPath,
  isSettingsReady,
  normalizeNewlines,
  suggestionTone,
  summarizeChunkSuggestions
} from "../lib/helpers";
import { buildDiffHunks, diffTextByLines } from "../lib/diff";
import { ActionButton } from "../components/ActionButton";
import { Panel } from "../components/Panel";
import { StatusBadge } from "../components/StatusBadge";

type DocumentView = "markup" | "source" | "final";
type EditorReviewView = "diff" | "source" | "current";

const DOCUMENT_VIEW_OPTIONS: ReadonlyArray<{
  key: DocumentView;
  label: string;
  hint: string;
}> = [
  { key: "markup", label: "修订标记", hint: "查看插入/删除标记与高亮" },
  { key: "final", label: "修改后", hint: "按当前最新候选合并成整篇" },
  { key: "source", label: "修改前", hint: "查看原文整篇（不含任何改写）" }
];

const EDITOR_REVIEW_OPTIONS: ReadonlyArray<{
  key: EditorReviewView;
  label: string;
}> = [
  { key: "diff", label: "Diff" },
  { key: "source", label: "原文" },
  { key: "current", label: "当前" }
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
  editorMode: boolean;
  editorText: string;
  editorDirty: boolean;
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
  onEnterEditor: () => void;
  onChangeEditorText: (value: string) => void;
  onSaveEditor: () => void;
  onSaveEditorAndExit: () => void;
  onDiscardEditorChanges: () => void;
  onExitEditor: () => void;
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
  editorMode,
  editorText,
  editorDirty,
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
  onOpenSettings,
  onEnterEditor,
  onChangeEditorText,
  onSaveEditor,
  onSaveEditorAndExit,
  onDiscardEditorChanges,
  onExitEditor
}: WorkbenchStageProps) {
  const settingsReady = isSettingsReady(settings);
  const [documentView, setDocumentView] = useState<DocumentView>("markup");
  const [copyState, setCopyState] = useState<CopyState>("idle");
  const [editorReviewView, setEditorReviewView] = useState<EditorReviewView>("diff");
  const [activeEditorHunkId, setActiveEditorHunkId] = useState<string | null>(null);
  const copyResetTimerRef = useRef<number | null>(null);
  const editorFieldRef = useRef<HTMLDivElement | null>(null);
  const editorDiffViewRef = useRef<HTMLDivElement | null>(null);
  const rewriteRunning = currentSession?.status === "running";
  const rewritePaused = currentSession?.status === "paused";
  const docxDocument = Boolean(
    currentSession && isDocxPath(currentSession.documentPath)
  );
  const anyBusy = Boolean(busyAction);
  const saveBusy = busyAction === "save-edits";
  const saveAndExitBusy = busyAction === "save-edits-and-back";
  const deferredEditorText = useDeferredValue(editorText);

  useEffect(() => {
    if (editorMode) return;
    setEditorReviewView("diff");
    setActiveEditorHunkId(null);
  }, [editorMode]);

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
  const showCancelAction = rewriteRunning || rewritePaused;
  const hasAppliedEdits = Boolean(currentStats && currentStats.suggestionsApplied > 0);

  const canEnterEditor = Boolean(
    currentSession &&
      !docxDocument &&
      !rewriteRunning &&
      !rewritePaused &&
      currentSession.status === "idle" &&
      currentSession.suggestions.length === 0 &&
      currentSession.chunks.every(
        (chunk) => chunk.status === "idle" || chunk.skipRewrite
      ) &&
      !anyBusy
  );

  const enterEditorTitle = useMemo(() => {
    if (!currentSession) return "请先打开一个文档";
    if (docxDocument) {
      return "docx 目前仅支持导入/改写/导出，暂不支持终稿编辑或写回覆盖";
    }
    if (rewriteRunning || rewritePaused) {
      return "请先取消自动任务后再编辑终稿";
    }
    if (anyBusy) return "当前有操作在执行，请稍后再试";
    if (currentSession.status !== "idle") {
      return "当前文档状态不是空闲，暂不可编辑终稿";
    }
    if (currentSession.suggestions.length > 0) {
      return "该文档存在修订记录，为避免冲突，请先“覆写并清理记录”或“重置记录”后再编辑";
    }
    if (currentSession.chunks.some((chunk) => !chunk.skipRewrite && chunk.status !== "idle")) {
      return "该文档存在生成进度/失败片段，为避免冲突，请先“覆写并清理记录”或“重置记录”后再编辑";
    }
    return "编辑终稿（仅在无修订记录时开放）";
  }, [anyBusy, currentSession, docxDocument, rewritePaused, rewriteRunning]);

  const finalizeDisabled =
    finalizeBusy ||
    (anyBusy && busyAction !== "finalize-document") ||
    rewriteRunning ||
    rewritePaused ||
    !hasAppliedEdits ||
    docxDocument;

  const finalizeTitle = useMemo(() => {
    if (finalizeBusy) return "正在写回原文件…";
    if (docxDocument) {
      return "docx 暂不支持写回覆盖，请导出为纯文本后再写回";
    }
    if (rewriteRunning || rewritePaused) {
      return "请先取消自动任务后再写回原文件";
    }
    if (!currentStats) return "正在计算会话状态…";
    if (currentStats.suggestionsApplied === 0) {
      return "还没有已应用的修改（先在右侧点“应用”）";
    }
    return "覆盖原文件并清理记录（不可撤销）";
  }, [currentStats, docxDocument, finalizeBusy, rewritePaused, rewriteRunning]);

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
    if (editorMode) {
      return "编辑终稿";
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
  }, [currentSession, documentView, editorMode]);

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
    if (editorMode) return editorText;
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
  }, [currentSession, documentView, editorMode, editorText, suggestionsByChunk]);

  const canCopy = copyText != null;

  const copyTitle = useMemo(() => {
    if (editorMode) return "复制当前编辑内容";
    if (documentView === "source") return "复制修改前全文";
    if (documentView === "final") return "复制修改后全文";
    return "切换到「修改前 / 修改后」后可复制";
  }, [documentView, editorMode]);

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
  }, [copyText, writeClipboardText]);

  const editorCharacterCount = useMemo(
    () => (editorMode ? countCharacters(editorText) : 0),
    [editorMode, editorText]
  );

  useEffect(() => {
    if (!editorMode) return;

    const handleKeyDown = (event: KeyboardEvent) => {
      const key = event.key.toLowerCase();
      const saveCombo = (event.ctrlKey || event.metaKey) && key === "s";
      if (!saveCombo) return;

      event.preventDefault();
      if (!editorDirty || anyBusy) return;
      onSaveEditor();
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [anyBusy, editorDirty, editorMode, onSaveEditor]);

  useEffect(() => {
    if (!editorMode) return;
    const node = editorFieldRef.current;
    if (!node) return;

    const domText = normalizeNewlines(node.innerText);
    if (domText === editorText) return;
    if (document.activeElement === node && editorDirty) return;

    node.innerText = editorText;
  }, [editorDirty, editorMode, editorText]);

  useEffect(() => {
    if (!editorMode) return;
    const node = editorFieldRef.current;
    if (!node) return;

    requestAnimationFrame(() => {
      node.focus();
    });
  }, [editorMode]);

  const handleEditorInput = useCallback(() => {
    const node = editorFieldRef.current;
    if (!node) return;
    onChangeEditorText(normalizeNewlines(node.innerText));
  }, [onChangeEditorText]);

  const handleEditorPaste = useCallback((event: ClipboardEvent<HTMLDivElement>) => {
    event.preventDefault();
    const text = event.clipboardData.getData("text/plain");
    if (!text) return;

    const ok = document.execCommand("insertText", false, text);
    if (ok) return;

    const selection = window.getSelection();
    if (!selection || selection.rangeCount === 0) return;
    selection.deleteFromDocument();
    selection.getRangeAt(0).insertNode(document.createTextNode(text));
    selection.collapseToEnd();
  }, []);

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

  const editorDiffSpans = useMemo(() => {
    if (!editorMode) return [];
    if (!currentSession) return [];
    return diffTextByLines(currentSession.sourceText, deferredEditorText);
  }, [currentSession, deferredEditorText, editorMode]);

  const editorDiffStats = useMemo(() => {
    let inserted = 0;
    let deleted = 0;
    for (const span of editorDiffSpans) {
      if (span.type === "insert") inserted += countCharacters(span.text);
      if (span.type === "delete") deleted += countCharacters(span.text);
    }
    return { inserted, deleted };
  }, [editorDiffSpans]);

  const editorHunks = useMemo(
    () => (editorMode ? buildDiffHunks(editorDiffSpans) : []),
    [editorDiffSpans, editorMode]
  );

  const activeEditorHunk = useMemo(() => {
    if (!editorMode) return null;
    if (editorHunks.length === 0) return null;
    return (
      editorHunks.find((item) => item.id === activeEditorHunkId) ?? editorHunks[0]
    );
  }, [activeEditorHunkId, editorHunks, editorMode]);

  useEffect(() => {
    if (!editorMode) return;
    if (editorHunks.length === 0) {
      if (activeEditorHunkId !== null) {
        setActiveEditorHunkId(null);
      }
      return;
    }
    if (!activeEditorHunkId || !editorHunks.some((item) => item.id === activeEditorHunkId)) {
      setActiveEditorHunkId(editorHunks[0].id);
    }
  }, [activeEditorHunkId, editorHunks, editorMode]);

  useEffect(() => {
    if (!editorMode) return;
    const node = editorDiffViewRef.current;
    if (!node) return;
    node.scrollTop = 0;
  }, [activeEditorHunk?.id, editorMode, editorReviewView]);

  return (
    <div className="workbench-root">
      <div className="workbench-layout">
        <div className="workbench-column is-center">
          <Panel
            title="文档"
            subtitle={documentSubtitle}
            className="workbench-doc-panel"
            bodyClassName="workbench-center-body"
            action={
              currentSession ? (
                <div className="workbench-doc-actionbar">
                  <div
                    className="workbench-doc-actionbar-left"
                    aria-label="文档视图与编辑状态"
                  >
                    <div
                      className={`workbench-action-reel workbench-view-reel ${
                        editorMode ? "is-editor" : ""
                      }`}
                    >
                      <div className="workbench-action-track">
                        <div className="workbench-action-row is-normal" aria-hidden={editorMode}>
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
                              disabled={editorMode}
                            >
                              {option.label}
                            </button>
                          ))}
                        </div>

                        <div className="workbench-action-row is-editor" aria-hidden={!editorMode}>
                          <span className="editor-chip">编辑模式</span>
                          <span className="editor-chip">
                            {editorDirty ? "未保存" : "已保存"}
                          </span>
                          <span className="editor-chip" title="字符数（不含空白）">
                            字符：{editorCharacterCount}
                          </span>
                        </div>
                      </div>
                    </div>
                  </div>

                  <div className="workbench-doc-actionbar-right">
                    <div className={`workbench-action-reel ${editorMode ? "is-editor" : ""}`}>
                      <div className="workbench-action-track">
                        <div className="workbench-action-row is-normal" aria-hidden={editorMode}>
                          <button
                            type="button"
                            className="icon-button"
                            onClick={() => void handleCopyDocument()}
                            aria-label={canCopy ? copyTitle : "复制（当前视图不可用）"}
                            title={copyTitle}
                            disabled={!canCopy || copyState === "copying" || editorMode}
                          >
                            {!canCopy ? (
                              <Copy />
                            ) : copyState === "copying" ? (
                              <LoaderCircle className="spin" />
                            ) : copyState === "done" ? (
                              <Check />
                            ) : copyState === "error" ? (
                              <AlertCircle />
                            ) : (
                              <Copy />
                            )}
                          </button>

                          <button
                            type="button"
                            className="icon-button"
                            onClick={onEnterEditor}
                            aria-label="进入编辑模式"
                            title={enterEditorTitle}
                            disabled={!canEnterEditor || editorMode}
                          >
                            <FilePenLine />
                          </button>

                          <button
                            type="button"
                            className="icon-button"
                            onClick={onResetSession}
                            aria-label="重置该文档记录（不修改原文件）"
                            title="重置该文档记录（不修改原文件）"
                            disabled={
                              editorMode ||
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
                            disabled={editorMode || finalizeDisabled}
                          >
                            {finalizeBusy ? <LoaderCircle className="spin" /> : <FileCheck2 />}
                          </button>

                          <button
                            type="button"
                            className={`icon-button ${showCancelAction ? "" : "is-placeholder"}`}
                            onClick={onCancel}
                            aria-label="取消执行"
                            title="取消"
                            aria-hidden={!showCancelAction}
                            tabIndex={showCancelAction ? 0 : -1}
                            disabled={
                              editorMode ||
                              !showCancelAction ||
                              cancelBusy ||
                              (anyBusy && busyAction !== "cancel-rewrite")
                            }
                          >
                            {cancelBusy ? <LoaderCircle className="spin" /> : <Square />}
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
                              editorMode ||
                              (rewriteRunning
                                ? pauseBusy || (anyBusy && busyAction !== runKey)
                                : rewritePaused
                                  ? resumeBusy || (anyBusy && busyAction !== runKey)
                                  : !canStartRewrite ||
                                    startBusy ||
                                    (anyBusy && busyAction !== runKey))
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
                        </div>

                        <div className="workbench-action-row is-editor" aria-hidden={!editorMode}>
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

                          <button
                            type="button"
                            className={`icon-button is-danger ${editorDirty ? "" : "is-placeholder"}`}
                            onClick={onDiscardEditorChanges}
                            aria-label="放弃未保存修改"
                            title={anyBusy ? "当前有操作在执行，请稍后再试" : "放弃未保存修改"}
                            aria-hidden={!editorDirty}
                            tabIndex={editorDirty ? 0 : -1}
                            disabled={!editorDirty || anyBusy}
                          >
                            <Undo2 />
                          </button>

                          <button
                            type="button"
                            className="toolbar-button is-primary"
                            onClick={() => {
                              if (editorDirty) {
                                onSaveEditorAndExit();
                                return;
                              }
                              onExitEditor();
                            }}
                            aria-label={editorDirty ? "保存并退出编辑模式" : "返回工作台"}
                            title={
                              editorDirty
                                ? saveAndExitBusy
                                  ? "正在写回原文件…"
                                  : anyBusy
                                    ? "当前有操作在执行，请稍后再试"
                                    : "保存并返回工作台"
                                : anyBusy
                                  ? "当前有操作在执行，请稍后再试"
                                  : "返回工作台"
                            }
                            disabled={
                              editorDirty
                                ? saveAndExitBusy || (anyBusy && !saveAndExitBusy)
                                : anyBusy
                            }
                          >
                            {editorDirty ? (
                              saveAndExitBusy ? (
                                <LoaderCircle className="spin" />
                              ) : (
                                <Save />
                              )
                            ) : (
                              <ArrowLeft />
                            )}
                            <span>{editorDirty ? "保存并退出" : "返回工作台"}</span>
                          </button>
                        </div>
                      </div>
                    </div>
                  </div>
                </div>
              ) : null
            }
          >
            {currentSession ? (
              <article className="editor-paper workbench-editor-paper">
                <div className="paper-content scroll-region">
                  {editorMode ? (
                    <div
                      ref={editorFieldRef}
                      className={`document-flow workbench-editor-editable ${
                        editorText.trim().length === 0 ? "is-empty" : ""
                      }`}
                      contentEditable
                      role="textbox"
                      aria-multiline="true"
                      aria-label="编辑终稿"
                      tabIndex={0}
                      spellCheck={false}
                      data-placeholder="在此编辑终稿…"
                      onInput={handleEditorInput}
                      onPaste={handleEditorPaste}
                      suppressContentEditableWarning
                    />
                  ) : (
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
                  )}
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
            subtitle="修改对时间线"
            className="workbench-review-panel"
            bodyClassName="workbench-review-body"
            action={
              <div className={`workbench-action-reel ${editorMode ? "is-editor" : ""}`}>
                <div className="workbench-action-track">
                  <div
                    className="workbench-review-actionbar workbench-action-row is-normal"
                    aria-hidden={editorMode}
                  >
                    <div className="workbench-review-actionbar-status">
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
                    </div>

                    <div className="workbench-review-actionbar-buttons">
                      {currentSession && activeChunk?.status === "failed" ? (
                        <button
                          type="button"
                          className="icon-button icon-button-sm"
                          onClick={onRetry}
                          aria-label="重试生成当前位置"
                          title="重试生成当前位置"
                          disabled={
                            editorMode ||
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
                              editorMode ||
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
                              editorMode ||
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
                              editorMode ||
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

                  <div
                    className="workbench-review-actionbar workbench-action-row is-editor"
                    aria-hidden={!editorMode}
                  >
                    <div className="workbench-review-actionbar-status">
                      <StatusBadge tone="info">编辑模式</StatusBadge>
                    </div>
                    <div className="workbench-review-actionbar-buttons">
                      <StatusBadge tone="info">审阅只读</StatusBadge>
                    </div>
                  </div>
                </div>
              </div>
            }
          >
            {currentSession && currentStats ? (
              editorMode ? (
                <>
                  <div className="context-group">
                    <span className="context-chip">
                      手动编辑：{editorDirty ? "未保存" : "已保存"}
                    </span>
                    <span className="context-chip">
                      变更：+{editorDiffStats.inserted} -{editorDiffStats.deleted}
                    </span>
                    <span className="context-chip">变更对：{editorHunks.length}</span>
                  </div>

                  {activeEditorHunk ? (
                    <>
                      <div className="review-switches">
                        {EDITOR_REVIEW_OPTIONS.map((item) => (
                          <button
                            key={item.key}
                            type="button"
                            className={`switch-chip ${
                              editorReviewView === item.key ? "is-active" : ""
                            }`}
                            onClick={() => setEditorReviewView(item.key)}
                          >
                            {item.label}
                          </button>
                        ))}
                      </div>

                      <div className="diff-view" ref={editorDiffViewRef}>
                        {editorReviewView === "diff" ? (
                          <p>
                            {activeEditorHunk.diffSpans.map((span, index) => (
                              <span
                                key={`${span.type}-${index}-${span.text.length}`}
                                className={`diff-span is-${span.type}`}
                              >
                                {span.text}
                              </span>
                            ))}
                          </p>
                        ) : null}

                        {editorReviewView === "source" ? (
                          <p>{activeEditorHunk.beforeText}</p>
                        ) : null}

                        {editorReviewView === "current" ? (
                          <p>{activeEditorHunk.afterText}</p>
                        ) : null}
                      </div>

                      <div className="suggestion-list scroll-region">
                        {editorHunks.map((hunk) => {
                          const preview =
                            hunk.afterText.trim().replace(/\s+/g, " ").slice(0, 24) ||
                            "（空变更）";
                          const more = hunk.afterText.trim().length > 24 ? "…" : "";

                          return (
                            <button
                              key={hunk.id}
                              type="button"
                              className={`suggestion-row ${
                                hunk.id === activeEditorHunk.id ? "is-active" : ""
                              }`}
                              onClick={() => setActiveEditorHunkId(hunk.id)}
                            >
                              <div className="suggestion-row-head">
                                <strong>
                                  #{hunk.sequence} · {preview}
                                  {more}
                                </strong>
                                <StatusBadge tone="info">
                                  +{hunk.insertedChars} -{hunk.deletedChars}
                                </StatusBadge>
                              </div>
                              <div className="suggestion-row-meta">
                                <span>片段：{countCharacters(hunk.afterText)} 字</span>
                              </div>
                              <p className="suggestion-row-preview">{hunk.afterText}</p>
                            </button>
                          );
                        })}
                      </div>
                    </>
                  ) : (
                    <div className="empty-inline">
                      <span>暂无变更。</span>
                    </div>
                  )}
                </>
              ) : (
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

                      {reviewView === "source" ? <p>{activeSuggestion.beforeText}</p> : null}

                      {reviewView === "candidate" ? <p>{activeSuggestion.afterText}</p> : null}
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
              )
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
