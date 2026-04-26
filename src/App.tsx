import {
  type PointerEvent as ReactPointerEvent,
  startTransition,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState
} from "react";
import { loadSession, loadSettings } from "./lib/api";
import { DEFAULT_SETTINGS } from "./lib/constants";
import { applyEditorSlotOverride, buildEditorTextFromSession, type EditorSlotOverrides } from "./lib/editorSlots";
import { documentEditorMode } from "./lib/documentCapabilities";
import {
  canRewriteSession,
  findRewriteUnit,
  getLatestSuggestion,
  getSessionStats,
  isSettingsReady,
  normalizeNewlines,
  readableError
} from "./lib/helpers";
import { normalizeSelectedRewriteUnitIds } from "./lib/rewriteUnitSelection";
import { isWindowDragExcludedTarget } from "./lib/windowDrag";
import type {
  AppSettings,
  DocumentSession,
  DocumentSnapshot,
  ProviderCheckResult,
  RewriteProgress
} from "./lib/types";
import { useNotice } from "./hooks/useNotice";
import { useBusyAction } from "./hooks/useBusyAction";
import { useTauriEvents } from "./hooks/useTauriEvents";
import { ConfirmModal } from "./components/ConfirmModal";
import { SettingsModal } from "./components/SettingsModal";
import { BootScreen } from "./app/components/BootScreen";
import { NoticeToast } from "./app/components/NoticeToast";
import { ThemeToggle } from "./app/components/ThemeToggle";
import { WindowResizeLayer } from "./app/components/WindowResizeLayer";
import { WorkspaceBar } from "./app/components/WorkspaceBar";
import { useConfirmDialog } from "./app/hooks/useConfirmDialog";
import { useUpdateChecker } from "./app/hooks/useUpdateChecker";
import { useSegmentationPresetLock } from "./app/hooks/useSegmentationPresetLock";
import { useDocumentActions } from "./app/hooks/useDocumentActions";
import { useDocumentFinalizeActions } from "./app/hooks/useDocumentFinalizeActions";
import { useDocumentScrollRestore } from "./app/hooks/useDocumentScrollRestore";
import { logScrollRestore } from "./app/hooks/documentScrollRestoreDebug";
import { useEditorSelectionRewrite } from "./app/hooks/useEditorSelectionRewrite";
import { useSettingsHandlers } from "./app/hooks/useSettingsHandlers";
import { useRewriteActions } from "./app/hooks/useRewriteActions";
import { useSuggestionActions } from "./app/hooks/useSuggestionActions";
import { useWindowControls } from "./app/hooks/useWindowControls";
import { resolveNextRewriteUnitId } from "./app/hooks/sessionActionShared";
import { WorkbenchStage } from "./stages/WorkbenchStage";
import type { DocumentEditorHandle } from "./stages/workbench/document/DocumentEditor";
import { isDesktopRuntime } from "./lib/runtimeMode";
import logoUrl from "../src-tauri/icons/lessai-logo.svg";

type ThemeMode = "light" | "dark";
type ThemePreference = ThemeMode | "system";

const LEGACY_THEME_STORAGE_KEY = "lessai.theme";
const THEME_PREFERENCE_STORAGE_KEY = "lessai.theme-preference";
const DARK_THEME_MEDIA_QUERY = "(prefers-color-scheme: dark)";

function resolveSystemThemeMode(): ThemeMode {
  if (typeof window === "undefined") {
    return "light";
  }

  if (
    typeof window.matchMedia === "function" &&
    window.matchMedia(DARK_THEME_MEDIA_QUERY).matches
  ) {
    return "dark";
  }

  return "light";
}

function resolveInitialThemePreference(): ThemePreference {
  if (typeof window === "undefined") {
    return "system";
  }

  try {
    const storedThemePreference = window.localStorage.getItem(THEME_PREFERENCE_STORAGE_KEY);
    if (
      storedThemePreference === "system" ||
      storedThemePreference === "light" ||
      storedThemePreference === "dark"
    ) {
      return storedThemePreference;
    }
  } catch {
    // Ignore storage errors and fall back to system preference.
  }

  return "system";
}

export default function App() {
  const desktopRuntime = isDesktopRuntime();
  const [stage, setStage] = useState<"workbench" | "editor">("workbench");
  const [booting, setBooting] = useState(true);
  const [themePreference, setThemePreference] =
    useState<ThemePreference>(() => resolveInitialThemePreference());
  const [systemThemeMode, setSystemThemeMode] =
    useState<ThemeMode>(() => resolveSystemThemeMode());
  const [settings, setSettings] = useState<AppSettings>(DEFAULT_SETTINGS);
  const [currentSession, setCurrentSession] = useState<DocumentSession | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [activeRewriteUnitId, setActiveRewriteUnitId] = useState<string | null>(null);
  const [activeSuggestionId, setActiveSuggestionId] = useState<string | null>(null);
  const [activeReviewNavigationRequestId, setActiveReviewNavigationRequestId] = useState(0);
  const [selectedRewriteUnitIds, setSelectedRewriteUnitIds] = useState<string[]>([]);
  const [providerStatus, setProviderStatus] =
    useState<ProviderCheckResult | null>(null);
  const [liveProgress, setLiveProgress] = useState<RewriteProgress | null>(null);
  const [editorBaselineText, setEditorBaselineText] = useState("");
  const [editorText, setEditorText] = useState("");
  const [editorSlotOverrides, setEditorSlotOverrides] = useState<EditorSlotOverrides>({});
  const [editorHasSelection, setEditorHasSelection] = useState(false);

  const { notice, showNotice, dismissNotice } = useNotice();
  const { busyAction, withBusy } = useBusyAction();
  const { confirmDialog, requestConfirm, handleConfirmResult } = useConfirmDialog();
  const { documentScrollRef, captureDocumentScrollPosition, restoreDocumentScrollPosition } =
    useDocumentScrollRestore();
  const {
    windowMaximized,
    customResizeEnabled,
    handleStartWindowDrag,
    handleMinimizeWindow,
    handleToggleMaximizeWindow,
    handleCloseWindow,
    handleResizeWindow
  } = useWindowControls(showNotice);
  const {
    currentVersion,
    releaseVersions,
    selectedReleaseTag,
    selectedRelease,
    selectedReleaseIsCurrent,
    releaseListLoadedAt,
    switchRequiresUpdaterManifest,
    handleCheckUpdate,
    handleRefreshReleaseVersions,
    handleSelectReleaseTag,
    handleSwitchSelectedRelease
  } = useUpdateChecker({
    updateProxy: settings.updateProxy,
    showNotice,
    dismissNotice,
    requestConfirm,
    withBusy
  });

  const stageRef = useRef(stage);
  stageRef.current = stage;
  const currentSessionRef = useRef(currentSession);
  currentSessionRef.current = currentSession;
  const activeRewriteUnitIdRef = useRef(activeRewriteUnitId);
  activeRewriteUnitIdRef.current = activeRewriteUnitId;
  const activeSuggestionIdRef = useRef(activeSuggestionId);
  activeSuggestionIdRef.current = activeSuggestionId;
  const selectedRewriteUnitIdsRef = useRef(selectedRewriteUnitIds);
  selectedRewriteUnitIdsRef.current = selectedRewriteUnitIds;
  const editorTextRef = useRef(editorText);
  editorTextRef.current = editorText;
  const editorBaselineTextRef = useRef(editorBaselineText);
  editorBaselineTextRef.current = editorBaselineText;
  const editorBaseSnapshotRef = useRef<DocumentSnapshot | null>(null);
  const editorSlotOverridesRef = useRef(editorSlotOverrides);
  editorSlotOverridesRef.current = editorSlotOverrides;
  const editorRef = useRef<DocumentEditorHandle | null>(null);

  const editorDirty = editorText !== editorBaselineText;
  const editorDirtyRef = useRef(editorDirty);
  editorDirtyRef.current = editorDirty;

  const handleChangeEditorText = useCallback((value: string) => {
    setEditorText(normalizeNewlines(value));
  }, []);

  const handleChangeEditorSlotText = useCallback((slotId: string, value: string) => {
    const session = currentSessionRef.current;
    if (!session || documentEditorMode(session) !== "slotBased") return;
    const slot = session.writebackSlots.find((item) => item.id === slotId);
    if (!slot || !slot.editable) return;

    const normalized = normalizeNewlines(value);
    const nextOverrides = applyEditorSlotOverride(
      editorSlotOverridesRef.current,
      slot,
      normalized
    );

    setEditorSlotOverrides(nextOverrides);
    setEditorText(buildEditorTextFromSession(session, nextOverrides));
  }, []);

  const currentStats = useMemo(
    () => (currentSession ? getSessionStats(currentSession) : null),
    [currentSession]
  );
  const themeMode = themePreference === "system" ? systemThemeMode : themePreference;

  const activeRewriteUnit = useMemo(
    () => (currentSession ? findRewriteUnit(currentSession, activeRewriteUnitId) : null),
    [activeRewriteUnitId, currentSession]
  );

  const topbarProgress = useMemo(
    () =>
      currentSession && currentStats
        ? `${currentStats.unitsApplied}/${currentStats.total}`
        : "0/0",
    [currentSession, currentStats]
  );

  const settingsReady = isSettingsReady(settings);

  const handleWindowDragPointerDown = useCallback(
    (event: ReactPointerEvent<HTMLElement>) => {
      if (!desktopRuntime) {
        return;
      }

      if (event.button !== 0 || !event.isPrimary) {
        return;
      }

      if (isWindowDragExcludedTarget(event.target)) {
        return;
      }

      void handleStartWindowDrag();
    },
    [desktopRuntime, handleStartWindowDrag]
  );

  const { segmentationPresetLock, readSegmentationPresetLockedReason } = useSegmentationPresetLock({
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
      rewriteUnitId: string | null,
      preferredSuggestionId?: string | null
    ) => {
      if (preferredSuggestionId) {
        const exists = session.suggestions.some((item) => item.id === preferredSuggestionId);
        if (exists) return preferredSuggestionId;
      }

      if (rewriteUnitId) {
        let latestForRewriteUnit: { id: string; sequence: number } | null = null;
        for (const suggestion of session.suggestions) {
          if (suggestion.rewriteUnitId !== rewriteUnitId) continue;
          if (!latestForRewriteUnit || suggestion.sequence > latestForRewriteUnit.sequence) {
            latestForRewriteUnit = { id: suggestion.id, sequence: suggestion.sequence };
          }
        }
        if (latestForRewriteUnit) {
          return latestForRewriteUnit.id;
        }
      }

      return getLatestSuggestion(session)?.id ?? null;
    },
    []
  );

  const applySessionState = useCallback(
    (
      session: DocumentSession,
      nextRewriteUnitId: string | null,
      options?: {
        preferredSuggestionId?: string | null;
        preservedScrollTop?: number | null;
      }
    ) => {
      const resolvedRewriteUnitId =
        resolveNextRewriteUnitId(session, nextRewriteUnitId);
      const suggestionId = pickActiveSuggestionId(
        session,
        resolvedRewriteUnitId,
        options?.preferredSuggestionId ?? null
      );

      startTransition(() => {
        setCurrentSession(session);
        setActiveRewriteUnitId(resolvedRewriteUnitId);
        setActiveSuggestionId(suggestionId);
      });
      if (options && "preservedScrollTop" in options) {
        logScrollRestore("apply-session-state", {
          sessionId: session.id,
          nextRewriteUnitId: resolvedRewriteUnitId,
          preservedScrollTop: options.preservedScrollTop ?? null
        });
        restoreDocumentScrollPosition(options.preservedScrollTop ?? null);
      }
    },
    [pickActiveSuggestionId, restoreDocumentScrollPosition]
  );

  const refreshSessionState = useCallback(
    async (
      sessionId: string,
      options?: {
        preserveRewriteUnit?: boolean;
        preferredRewriteUnitId?: string | null;
        preserveSuggestion?: boolean;
        preferredSuggestionId?: string | null;
        preserveScroll?: boolean;
      }
    ) => {
      const preservedScrollTop =
        options?.preserveScroll === false ? undefined : captureDocumentScrollPosition();
      logScrollRestore("refresh-session-state-start", {
        sessionId,
        options: options ?? null,
        preservedScrollTop,
        activeRewriteUnitId: activeRewriteUnitIdRef.current,
        activeSuggestionId: activeSuggestionIdRef.current
      });
      const session = await loadSession(sessionId);
      const currentRewriteUnitId = activeRewriteUnitIdRef.current;
      const nextRewriteUnitId =
        options?.preferredRewriteUnitId ??
        (options?.preserveRewriteUnit &&
        currentRewriteUnitId &&
        session.rewriteUnits.some((item) => item.id === currentRewriteUnitId)
          ? currentRewriteUnitId
          : resolveNextRewriteUnitId(session));

      const preferredSuggestionId =
        options?.preferredSuggestionId ??
        (options?.preserveSuggestion ? activeSuggestionIdRef.current : null);

      logScrollRestore("refresh-session-state-loaded", {
        sessionId,
        loadedSessionId: session.id,
        nextRewriteUnitId,
        preferredSuggestionId,
        preservedScrollTop
      });
      applySessionState(session, nextRewriteUnitId, {
        preferredSuggestionId,
        preservedScrollTop
      });
      return session;
    },
    [applySessionState, captureDocumentScrollPosition]
  );

  const openSettings = useCallback(() => {
    setSettingsOpen(true);
  }, []);

  const closeSettings = useCallback(() => {
    setSettingsOpen(false);
  }, []);

  const handleToggleTheme = useCallback(() => {
    setThemePreference((current) => {
      if (current === "system") {
        return themeMode === "dark" ? "light" : "dark";
      }
      return current === "dark" ? "light" : "dark";
    });
  }, [themeMode]);

  const requestRevealActiveRewriteUnit = useCallback(() => {
    setActiveReviewNavigationRequestId((current) => current + 1);
  }, []);

  useLayoutEffect(() => {
    document.documentElement.dataset.theme = themeMode;
  }, [themeMode]);

  useLayoutEffect(() => {
    const root = document.getElementById("root");
    if (!root) {
      return;
    }
    root.dataset.runtime = desktopRuntime ? "desktop" : "web";
  }, [desktopRuntime]);

  useEffect(() => {
    try {
      window.localStorage.removeItem(LEGACY_THEME_STORAGE_KEY);
      if (themePreference === "system") {
        window.localStorage.removeItem(THEME_PREFERENCE_STORAGE_KEY);
      } else {
        window.localStorage.setItem(THEME_PREFERENCE_STORAGE_KEY, themePreference);
      }
    } catch {
      // Ignore storage errors and keep the in-memory theme selection.
    }
  }, [themePreference]);

  useEffect(() => {
    if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
      return;
    }

    const mediaQuery = window.matchMedia(DARK_THEME_MEDIA_QUERY);
    const handleThemeChange = (event: MediaQueryListEvent) => {
      setSystemThemeMode(event.matches ? "dark" : "light");
    };

    setSystemThemeMode(mediaQuery.matches ? "dark" : "light");

    if (typeof mediaQuery.addEventListener === "function") {
      mediaQuery.addEventListener("change", handleThemeChange);
      return () => {
        mediaQuery.removeEventListener("change", handleThemeChange);
      };
    }

    mediaQuery.addListener(handleThemeChange);
    return () => {
      mediaQuery.removeListener(handleThemeChange);
    };
  }, []);

  useEffect(() => {
    if (
      currentSession &&
      liveProgress &&
      liveProgress.sessionId === currentSession.id &&
      !["running", "paused"].includes(currentSession.status)
    ) {
      setLiveProgress(null);
    }
  }, [currentSession, liveProgress]);

  useEffect(() => {
    if (!currentSession) return;
    logScrollRestore("app-active-review-target", {
      sessionId: currentSession.id,
      activeRewriteUnitId,
      activeSuggestionId,
      activeReviewNavigationRequestId
    });
  }, [activeRewriteUnitId, activeReviewNavigationRequestId, activeSuggestionId, currentSession]);

  useTauriEvents({
    onProgress: async (payload: RewriteProgress) => {
      setLiveProgress((current) => {
        if (!current || current.sessionId !== payload.sessionId) return payload;

        const sameUnitIds =
          current.runningUnitIds.length === payload.runningUnitIds.length &&
          current.runningUnitIds.every((value, index) => value === payload.runningUnitIds[index]);

        const unchanged =
          current.completedUnits === payload.completedUnits &&
          current.inFlight === payload.inFlight &&
          current.totalUnits === payload.totalUnits &&
          current.mode === payload.mode &&
          current.runningState === payload.runningState &&
          current.maxConcurrency === payload.maxConcurrency &&
          sameUnitIds;

        return unchanged ? current : payload;
      });
    },
    onRewriteUnitCompleted: async (payload) => {
      const session = currentSessionRef.current;
      if (session && payload.sessionId === session.id) {
        logScrollRestore("tauri-rewrite-unit-completed", {
          sessionId: payload.sessionId,
          rewriteUnitId: payload.rewriteUnitId,
          suggestionId: payload.suggestionId
        });
        await refreshSessionState(payload.sessionId, {
          preferredRewriteUnitId: payload.rewriteUnitId,
          preferredSuggestionId: payload.suggestionId
        });
      }
    },
    onFinished: async (payload) => {
      setLiveProgress((current) =>
        current?.sessionId === payload.sessionId ? null : current
      );
      const session = currentSessionRef.current;
      if (session && payload.sessionId === session.id) {
        logScrollRestore("tauri-finished", { sessionId: payload.sessionId });
        const refreshed = await refreshSessionState(payload.sessionId, {
          preserveRewriteUnit: true,
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
        logScrollRestore("tauri-failed", {
          sessionId: payload.sessionId,
          error: payload.error
        });
        await refreshSessionState(payload.sessionId, {
          preserveRewriteUnit: true,
          preserveSuggestion: true
        });
      }
    }
  });

  useEffect(() => {
    void (async () => {
      try {
        const storedSettings = await loadSettings();
        startTransition(() => {
          setSettings(storedSettings);
          setStage("workbench");
          setCurrentSession(null);
          setActiveRewriteUnitId(null);
          setActiveSuggestionId(null);
          setSettingsOpen(false);
          setEditorBaselineText("");
          setEditorText("");
          setEditorSlotOverrides({});
        });
      } catch (error) {
        console.error("[lessai::boot] load settings failed", error);
        showNotice("error", `初始化失败：${readableError(error)}`);
      } finally {
        setBooting(false);
      }
    })();
  }, [showNotice]);

  useEffect(() => {
    if (stage === "editor" && !currentSession) {
      setStage("workbench");
    }
  }, [currentSession, stage]);

  useEffect(() => {
    setSelectedRewriteUnitIds([]);
  }, [currentSession?.id]);

  useEffect(() => {
    if (!currentSession) return;
    if (!canRewriteSession(currentSession)) {
      setSelectedRewriteUnitIds([]);
      return;
    }
    setSelectedRewriteUnitIds((current) => {
      const normalized = normalizeSelectedRewriteUnitIds(currentSession, current);
      const unchanged =
        current.length === normalized.length &&
        current.every((value, index) => value === normalized[index]);
      return unchanged ? current : normalized;
    });
  }, [currentSession]);

  const {
    handleUpdateStringSetting,
    handleUpdateNumberSetting,
    handleUpdateSegmentationPreset,
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
    currentSession,
    showNotice,
    withBusy,
    closeSettings,
    readSegmentationPresetLockedReason,
    refreshSessionState
  });

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
  });

  const { handleRewriteSelection } = useEditorSelectionRewrite({
    stageRef,
    currentSessionRef,
    editorBaseSnapshotRef,
    editorRef,
    requestConfirm,
    showNotice,
    withBusy
  });

  const { handleExport, handleFinalizeDocument, handleResetSession } =
    useDocumentFinalizeActions({
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
    activeRewriteUnitIdRef,
    activeSuggestionIdRef,
    selectedRewriteUnitIdsRef,
    captureDocumentScrollPosition,
    editorDirtyRef,
    requestConfirm,
    applySessionState,
    refreshSessionState,
    setLiveProgress,
    showNotice,
    withBusy
  });

  const {
    handleSelectRewriteUnit,
    handleSelectSuggestion,
    handleApplySuggestion,
    handleDismissSuggestion,
    handleDeleteSuggestion
  } = useSuggestionActions({
    currentSessionRef,
    activeRewriteUnitIdRef,
    captureDocumentScrollPosition,
    requestRevealActiveRewriteUnit,
    setActiveRewriteUnitId,
    setActiveSuggestionId,
    setSelectedRewriteUnitIds,
    applySessionState,
    refreshSessionState,
    showNotice,
    withBusy
  });

  if (booting) {
    return <BootScreen />;
  }

  return (
    <div
      className={`app-shell${windowMaximized ? " is-maximized" : ""}${desktopRuntime ? " is-desktop-runtime" : " is-web-runtime"}`}
    >
      <div className="body-shell">
        <main className="workspace" onPointerDown={handleWindowDragPointerDown}>
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
            showWindowControls={desktopRuntime}
            enableWindowDrag={desktopRuntime}
            onOpenDocument={() => void handleOpenDocument()}
            onOpenSettings={openSettings}
            onExport={() => void handleExport()}
            onStartWindowDrag={() => void handleStartWindowDrag()}
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
              activeRewriteUnit={activeRewriteUnit}
              activeRewriteUnitId={activeRewriteUnitId}
              activeSuggestionId={activeSuggestionId}
              activeReviewNavigationRequestId={activeReviewNavigationRequestId}
              selectedRewriteUnitIds={selectedRewriteUnitIds}
              busyAction={busyAction}
              editorMode={stage === "editor"}
              editorText={editorText}
              editorSlotOverrides={editorSlotOverrides}
              editorDirty={editorDirty}
              editorHasSelection={editorHasSelection}
              editorRef={editorRef}
              documentScrollRef={documentScrollRef}
              onOpenDocument={handleOpenDocument}
              onSelectRewriteUnit={handleSelectRewriteUnit}
              onSelectSuggestion={handleSelectSuggestion}
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
              onChangeEditorSlotText={handleChangeEditorSlotText}
              onChangeEditorHasSelection={setEditorHasSelection}
              onSaveEditor={() => void handleSaveEditor()}
              onSaveEditorAndExit={() =>
                void handleSaveEditor({ returnToWorkbench: true })
              }
              onDiscardEditorChanges={handleDiscardEditorChanges}
              onExitEditor={handleExitEditor}
              onRewriteSelection={() => void handleRewriteSelection()}
            />
          </div>

          <NoticeToast notice={notice} onDismiss={dismissNotice} />

          <SettingsModal
            open={settingsOpen}
            settings={settings}
            providerStatus={providerStatus}
            busyAction={busyAction}
            segmentationPresetLocked={segmentationPresetLock.locked}
            segmentationPresetLockedReason={segmentationPresetLock.reason}
            onClose={closeSettings}
            onUpdateStringSetting={handleUpdateStringSetting}
            onUpdateNumberSetting={handleUpdateNumberSetting}
            onUpdateSegmentationPreset={handleUpdateSegmentationPreset}
            onUpdateRewriteHeadings={handleUpdateRewriteHeadings}
            onUpdateRewriteMode={handleUpdateRewriteMode}
            onUpdatePromptPresetId={handleUpdatePromptPresetId}
            onUpsertCustomPrompt={handleUpsertCustomPrompt}
            onDeleteCustomPrompt={handleDeleteCustomPrompt}
            currentVersion={currentVersion}
            releaseVersions={releaseVersions}
            selectedReleaseTag={selectedReleaseTag}
            selectedRelease={selectedRelease}
            selectedReleaseIsCurrent={selectedReleaseIsCurrent}
            releaseListLoadedAt={releaseListLoadedAt}
            switchRequiresUpdaterManifest={switchRequiresUpdaterManifest}
            onConfirm={requestConfirm}
            onTestProvider={handleTestProvider}
            onSaveSettings={handleSaveSettings}
            onCheckUpdate={() => void handleCheckUpdate()}
            onRefreshReleaseVersions={() => void handleRefreshReleaseVersions()}
            onSelectReleaseTag={handleSelectReleaseTag}
            onSwitchSelectedRelease={() => void handleSwitchSelectedRelease()}
          />
        </main>
      </div>

      <ThemeToggle themeMode={themeMode} onToggle={handleToggleTheme} />

      <ConfirmModal
        open={confirmDialog != null}
        title={confirmDialog?.title ?? ""}
        message={confirmDialog?.message ?? ""}
        okLabel={confirmDialog?.okLabel}
        cancelLabel={confirmDialog?.cancelLabel}
        variant={confirmDialog?.variant}
        onResult={handleConfirmResult}
      />
      {desktopRuntime && customResizeEnabled && !windowMaximized ? (
        <WindowResizeLayer onResize={handleResizeWindow} />
      ) : null}
    </div>
  );
}
