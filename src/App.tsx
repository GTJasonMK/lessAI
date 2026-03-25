import {
  startTransition,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState
} from "react";
import { loadSession, loadSettings } from "./lib/api";
import type {
  AppSettings,
  DocumentSession,
  PromptTemplate,
  ProviderCheckResult,
  RewriteProgress,
} from "./lib/types";
import { DEFAULT_SETTINGS } from "./lib/constants";
import type { ReviewView } from "./lib/constants";
import {
  formatDisplayPath,
  getLatestSuggestion,
  getSessionStats,
  isSettingsReady,
  normalizeNewlines,
  readableError,
  selectDefaultChunkIndex,
} from "./lib/helpers";
import { useNotice } from "./hooks/useNotice";
import { useBusyAction } from "./hooks/useBusyAction";
import { useTauriEvents } from "./hooks/useTauriEvents";
import { ConfirmModal } from "./components/ConfirmModal";
import { SettingsModal } from "./components/SettingsModal";
import { BootScreen } from "./app/components/BootScreen";
import { NoticeToast } from "./app/components/NoticeToast";
import { WindowResizeLayer } from "./app/components/WindowResizeLayer";
import { WorkspaceBar } from "./app/components/WorkspaceBar";
import { useConfirmDialog } from "./app/hooks/useConfirmDialog";
import { useUpdateChecker } from "./app/hooks/useUpdateChecker";
import { useChunkStrategyLock } from "./app/hooks/useChunkStrategyLock";
import { useDocumentActions } from "./app/hooks/useDocumentActions";
import { useDocumentFinalizeActions } from "./app/hooks/useDocumentFinalizeActions";
import { useSettingsHandlers } from "./app/hooks/useSettingsHandlers";
import { useRewriteActions } from "./app/hooks/useRewriteActions";
import { useSuggestionActions } from "./app/hooks/useSuggestionActions";
import { useWindowControls } from "./app/hooks/useWindowControls";
import { WorkbenchStage } from "./stages/WorkbenchStage";
import logoUrl from "../src-tauri/icons/lessai-logo.svg";

export default function App() {
  // ── 核心状态 ─────────────────────────────────────────

  const [stage, setStage] = useState<"workbench" | "editor">("workbench");
  const [booting, setBooting] = useState(true);
  const [settings, setSettings] = useState<AppSettings>(DEFAULT_SETTINGS);
  const [currentSession, setCurrentSession] = useState<DocumentSession | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [activeChunkIndex, setActiveChunkIndex] = useState(0);
  const [activeSuggestionId, setActiveSuggestionId] = useState<string | null>(null);
  const [reviewView, setReviewView] = useState<ReviewView>("diff");
  const [providerStatus, setProviderStatus] =
    useState<ProviderCheckResult | null>(null);
  const [liveProgress, setLiveProgress] = useState<RewriteProgress | null>(null);
  const [editorBaselineText, setEditorBaselineText] = useState("");
  const [editorText, setEditorText] = useState("");

  const { notice, showNotice, dismissNotice } = useNotice();
  const { busyAction, withBusy } = useBusyAction();
  const { confirmDialog, requestConfirm, handleConfirmResult } = useConfirmDialog();
  const {
    windowMaximized,
    handleMinimizeWindow,
    handleToggleMaximizeWindow,
    handleCloseWindow,
    handleResizeWindow
  } = useWindowControls(showNotice);
  const { handleCheckUpdate } = useUpdateChecker({
    updateProxy: settings.updateProxy,
    showNotice,
    dismissNotice,
    requestConfirm,
    withBusy
  });

  // 使用 ref 持有最新值，供事件回调读取，避免闭包捕获旧状态
  const stageRef = useRef(stage);
  stageRef.current = stage;
  const currentSessionRef = useRef(currentSession);
  currentSessionRef.current = currentSession;
  const activeChunkIndexRef = useRef(activeChunkIndex);
  activeChunkIndexRef.current = activeChunkIndex;
  const activeSuggestionIdRef = useRef(activeSuggestionId);
  activeSuggestionIdRef.current = activeSuggestionId;
  const editorTextRef = useRef(editorText);
  editorTextRef.current = editorText;
  const editorBaselineTextRef = useRef(editorBaselineText);
  editorBaselineTextRef.current = editorBaselineText;

  const editorDirty = editorText !== editorBaselineText;
  const editorDirtyRef = useRef(editorDirty);
  editorDirtyRef.current = editorDirty;

  const handleChangeEditorText = useCallback((value: string) => {
    setEditorText(normalizeNewlines(value));
  }, []);

  // ── 派生值（useMemo）────────────────────────────────

  const currentStats = useMemo(
    () => (currentSession ? getSessionStats(currentSession) : null),
    [currentSession]
  );

  const activeChunk = useMemo(
    () =>
      currentSession && currentSession.chunks[activeChunkIndex]
        ? currentSession.chunks[activeChunkIndex]
        : null,
    [currentSession, activeChunkIndex]
  );

  const topbarProgress = useMemo(
    () =>
      currentSession && currentStats
        ? `${currentStats.chunksApplied}/${currentStats.total}`
        : "0/0",
    [currentSession, currentStats]
  );

  const settingsReady = isSettingsReady(settings);

  const { chunkStrategyLock, readChunkStrategyLockedReason } = useChunkStrategyLock({
    stage,
    editorDirty,
    currentSession,
    stageRef,
    editorDirtyRef,
    currentSessionRef
  });

  const pickActiveSuggestionId = useCallback(
    (
      session: DocumentSession,
      chunkIndex: number,
      preferredSuggestionId?: string | null
    ) => {
      if (preferredSuggestionId) {
        const exists = session.suggestions.some((item) => item.id === preferredSuggestionId);
        if (exists) return preferredSuggestionId;
      }

      let latestForChunk: { id: string; sequence: number } | null = null;
      for (const suggestion of session.suggestions) {
        if (suggestion.chunkIndex !== chunkIndex) continue;
        if (!latestForChunk || suggestion.sequence > latestForChunk.sequence) {
          latestForChunk = { id: suggestion.id, sequence: suggestion.sequence };
        }
      }

      if (latestForChunk) {
        return latestForChunk.id;
      }

      const latestOverall = getLatestSuggestion(session);
      return latestOverall?.id ?? null;
    },
    []
  );

  const applySessionState = useCallback(
    (
      session: DocumentSession,
      nextChunkIndex: number,
      options?: { preferredSuggestionId?: string | null }
    ) => {
      const suggestionId = pickActiveSuggestionId(
        session,
        nextChunkIndex,
        options?.preferredSuggestionId ?? null
      );

      startTransition(() => {
        setCurrentSession(session);
        setActiveChunkIndex(nextChunkIndex);
        setActiveSuggestionId(suggestionId);
      });
    },
    [pickActiveSuggestionId]
  );

  const refreshSessionState = useCallback(
    async (
      sessionId: string,
      options?: {
        preserveChunk?: boolean;
        preferredChunkIndex?: number;
        preserveSuggestion?: boolean;
        preferredSuggestionId?: string | null;
      }
    ) => {
      const session = await loadSession(sessionId);
      const chunkIdx = activeChunkIndexRef.current;
      const nextChunkIndex =
        options?.preferredChunkIndex ??
        (options?.preserveChunk && chunkIdx < session.chunks.length
          ? chunkIdx
          : selectDefaultChunkIndex(session));

      const preferredSuggestionId =
        options?.preferredSuggestionId ??
        (options?.preserveSuggestion ? activeSuggestionIdRef.current : null);

      applySessionState(session, nextChunkIndex, { preferredSuggestionId });
      return session;
    },
    [applySessionState]
  );

  // ── Settings Modal ───────────────────────────────────

  const openSettings = useCallback(() => {
    setSettingsOpen(true);
  }, []);

  const closeSettings = useCallback(() => {
    setSettingsOpen(false);
  }, []);

  // ── liveProgress 清理（简化） ────────────────────────

  useEffect(() => {
    if (
      currentSession &&
      liveProgress &&
      liveProgress.sessionId === currentSession.id &&
      currentSession.status !== "running"
    ) {
      setLiveProgress(null);
    }
  }, [currentSession, liveProgress]);

  // ── Tauri 事件 ────────────────────────────────────────

  useTauriEvents({
    onProgress: async (payload: RewriteProgress) => {
      setLiveProgress(payload);
      // 只关心当前打开的文档；其他 session 的事件无需刷新列表（项目不再展示会话库）。
    },
    onChunkCompleted: async (payload) => {
      const session = currentSessionRef.current;
      if (session && payload.sessionId === session.id) {
        await refreshSessionState(payload.sessionId, {
          preferredChunkIndex: payload.index,
          preferredSuggestionId: payload.suggestionId
        });
        setReviewView("diff");
      }
    },
    onFinished: async (payload) => {
      setLiveProgress((current) =>
        current?.sessionId === payload.sessionId ? null : current
      );
      const session = currentSessionRef.current;
      if (session && payload.sessionId === session.id) {
        const refreshed = await refreshSessionState(payload.sessionId, {
          preserveChunk: true,
          preserveSuggestion: true
        });
        if (refreshed.status === "completed") {
          showNotice("success", "自动批处理已完成，当前文稿可以直接导出。");
        }
      }
    },
    onFailed: async (payload) => {
      setLiveProgress((current) =>
        current?.sessionId === payload.sessionId ? null : current
      );
      showNotice("error", `改写失败：${payload.error}`);
      const session = currentSessionRef.current;
      if (session && payload.sessionId === session.id) {
        const refreshed = await refreshSessionState(payload.sessionId, {
          preserveChunk: true,
          preserveSuggestion: true
        });
        if (refreshed.status === "failed") {
          setReviewView("diff");
        }
      }
    }
  });

  // ── 初始化 ────────────────────────────────────────────

  useEffect(() => {
    void (async () => {
      try {
        const storedSettings = await loadSettings();
        startTransition(() => {
          setSettings(storedSettings);
          setStage("workbench");
          setCurrentSession(null);
          setActiveChunkIndex(0);
          setActiveSuggestionId(null);
          // 默认先展示工作台，设置以弹窗形式按需打开。
          setSettingsOpen(false);
          setEditorBaselineText("");
          setEditorText("");
        });
      } catch (error) {
        showNotice("error", `初始化失败：${readableError(error)}`);
      } finally {
        setBooting(false);
      }
    })();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (stage === "editor" && !currentSession) {
      setStage("workbench");
    }
  }, [currentSession, stage]);

  // ── Settings handlers ────────────────────────────────
  const {
    handleUpdateStringSetting,
    handleUpdateNumberSetting,
    handleUpdateChunkPreset,
    handleUpdateRewriteHeadings,
    handleUpdateRewriteMode,
    handleUpdatePromptPresetId,
    handleUpsertCustomPrompt,
    handleDeleteCustomPrompt,
    handleSaveSettings,
    handleTestProvider
  } = useSettingsHandlers({
    settings,
    setSettings,
    setProviderStatus,
    showNotice,
    withBusy,
    closeSettings,
    readChunkStrategyLockedReason
  });

  // ── Document / Rewrite / Suggestion handlers ─────────

  const {
    handleOpenDocument,
    handleEnterEditor,
    handleDiscardEditorChanges,
    handleExitEditor,
    handleSaveEditor
  } = useDocumentActions({
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
  });

  const { handleExport, handleFinalizeDocument, handleResetSession } =
    useDocumentFinalizeActions({
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
    });

  const {
    handleStartRewrite,
    handlePause,
    handleResume,
    handleCancel: handleCancelRewrite,
    handleRetry
  } = useRewriteActions({
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
  });

  const {
    handleSelectChunk,
    handleSelectSuggestion,
    handleApplySuggestion,
    handleDismissSuggestion,
    handleDeleteSuggestion
  } = useSuggestionActions({
    currentSessionRef,
    activeChunkIndexRef,
    setActiveChunkIndex,
    setActiveSuggestionId,
    setReviewView,
    applySessionState,
    showNotice,
    withBusy
  });

  // ── 渲染 ─────────────────────────────────────────────

  if (booting) {
    return <BootScreen />;
  }

  return (
    <div className="app-shell">
      <div className="body-shell">
        <main className="workspace">
          <WorkspaceBar
            logoUrl={logoUrl}
            stage={stage}
            settingsOpen={settingsOpen}
            settingsReady={settingsReady}
            settings={settings}
            currentSession={currentSession}
            topbarProgress={topbarProgress}
            liveProgress={liveProgress}
            busyAction={busyAction}
            windowMaximized={windowMaximized}
            onOpenDocument={() => void handleOpenDocument()}
            onOpenSettings={openSettings}
            onExport={() => void handleExport()}
            onMinimizeWindow={() => void handleMinimizeWindow()}
            onToggleMaximizeWindow={() => void handleToggleMaximizeWindow()}
            onCloseWindow={() => void handleCloseWindow()}
          />

          <div className="workspace-stage">
            <WorkbenchStage
              settings={settings}
              currentSession={currentSession}
              liveProgress={liveProgress}
              currentStats={currentStats}
              activeChunk={activeChunk}
              activeChunkIndex={activeChunkIndex}
              activeSuggestionId={activeSuggestionId}
              reviewView={reviewView}
              busyAction={busyAction}
              editorMode={stage === "editor"}
              editorText={editorText}
              editorDirty={editorDirty}
              onOpenDocument={handleOpenDocument}
              onSelectChunk={handleSelectChunk}
              onSelectSuggestion={handleSelectSuggestion}
              onSetReviewView={setReviewView}
              onStartRewrite={(mode) => void handleStartRewrite(mode)}
              onPause={() => void handlePause()}
              onResume={() => void handleResume()}
              onCancel={() => void handleCancelRewrite()}
              onFinalizeDocument={() => void handleFinalizeDocument()}
              onResetSession={() => void handleResetSession()}
              onApplySuggestion={handleApplySuggestion}
              onDismissSuggestion={handleDismissSuggestion}
              onDeleteSuggestion={handleDeleteSuggestion}
              onRetry={handleRetry}
              onOpenSettings={openSettings}
              onEnterEditor={handleEnterEditor}
              onChangeEditorText={handleChangeEditorText}
              onSaveEditor={() => void handleSaveEditor()}
              onSaveEditorAndExit={() =>
                void handleSaveEditor({ returnToWorkbench: true })
              }
              onDiscardEditorChanges={handleDiscardEditorChanges}
              onExitEditor={handleExitEditor}
            />
          </div>

          <NoticeToast notice={notice} onDismiss={dismissNotice} />

          <SettingsModal
            open={settingsOpen}
            settings={settings}
            providerStatus={providerStatus}
            busyAction={busyAction}
            chunkStrategyLocked={chunkStrategyLock.locked}
            chunkStrategyLockedReason={chunkStrategyLock.reason}
            onClose={closeSettings}
            onUpdateStringSetting={handleUpdateStringSetting}
            onUpdateNumberSetting={handleUpdateNumberSetting}
            onUpdateChunkPreset={handleUpdateChunkPreset}
            onUpdateRewriteHeadings={handleUpdateRewriteHeadings}
            onUpdateRewriteMode={handleUpdateRewriteMode}
            onUpdatePromptPresetId={handleUpdatePromptPresetId}
            onUpsertCustomPrompt={handleUpsertCustomPrompt}
            onDeleteCustomPrompt={handleDeleteCustomPrompt}
            onConfirm={requestConfirm}
            onTestProvider={handleTestProvider}
            onSaveSettings={handleSaveSettings}
            onCheckUpdate={() => void handleCheckUpdate()}
          />
        </main>
      </div>

      <ConfirmModal
        open={confirmDialog != null}
        title={confirmDialog?.title ?? ""}
        message={confirmDialog?.message ?? ""}
        okLabel={confirmDialog?.okLabel}
        cancelLabel={confirmDialog?.cancelLabel}
        variant={confirmDialog?.variant}
        onResult={handleConfirmResult}
      />
      <WindowResizeLayer onResize={handleResizeWindow} />
    </div>
  );
}
