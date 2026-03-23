import {
  startTransition,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState
} from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";
import { BundleType, getBundleType, getVersion } from "@tauri-apps/api/app";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  Copy,
  Download,
  FolderOpen,
  LoaderCircle,
  Minus,
  Settings2,
  Square,
  X
} from "lucide-react";
import {
  applySuggestion,
  cancelRewrite,
  deleteSuggestion,
  dismissSuggestion,
  exportDocument,
  finalizeDocument,
  loadSession,
  loadSettings,
  openDocument,
  pauseRewrite,
  resetSession,
  resumeRewrite,
  retryChunk,
  saveDocumentEdits,
  saveSettings,
  startRewrite,
  testProvider
} from "./lib/api";
import type {
  AppSettings,
  DocumentSession,
  PromptTemplate,
  ProviderCheckResult,
  RewriteMode,
  RewriteProgress,
} from "./lib/types";
import { DEFAULT_SETTINGS } from "./lib/constants";
import type { ReviewView } from "./lib/constants";
import {
  formatSessionStatus,
  formatDisplayPath,
  formatBytes,
  getLatestSuggestion,
  getSessionStats,
  isDocxPath,
  isSettingsReady,
  normalizeNewlines,
  readableError,
  sanitizeFileName,
  selectDefaultChunkIndex,
  statusTone
} from "./lib/helpers";
import { useNotice } from "./hooks/useNotice";
import { useBusyAction } from "./hooks/useBusyAction";
import { useTauriEvents } from "./hooks/useTauriEvents";
import { StatusBadge } from "./components/StatusBadge";
import type { ConfirmModalOptions } from "./components/ConfirmModal";
import { ConfirmModal } from "./components/ConfirmModal";
import { SettingsModal } from "./components/SettingsModal";
import { WorkbenchStage } from "./stages/WorkbenchStage";
import logoUrl from "../src-tauri/icons/lessai-logo.svg";

type ResizeDirection =
  | "East"
  | "North"
  | "NorthEast"
  | "NorthWest"
  | "South"
  | "SouthEast"
  | "SouthWest"
  | "West";

const UPDATE_MANIFEST_URL =
  "https://github.com/GTJasonMK/lessAI/releases/latest/download/latest.json";

export default function App() {
  // ── 核心状态 ─────────────────────────────────────────

  const [stage, setStage] = useState<"workbench" | "editor">("workbench");
  const [booting, setBooting] = useState(true);
  const [windowMaximized, setWindowMaximized] = useState(false);
  const [settings, setSettings] = useState<AppSettings>(DEFAULT_SETTINGS);
  const [currentSession, setCurrentSession] = useState<DocumentSession | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [activeChunkIndex, setActiveChunkIndex] = useState(0);
  const [activeSuggestionId, setActiveSuggestionId] = useState<string | null>(null);
  const [reviewView, setReviewView] = useState<ReviewView>("diff");
  const [providerStatus, setProviderStatus] =
    useState<ProviderCheckResult | null>(null);
  const [liveProgress, setLiveProgress] = useState<RewriteProgress | null>(null);
  const [confirmDialog, setConfirmDialog] = useState<ConfirmModalOptions | null>(null);
  const [editorBaselineText, setEditorBaselineText] = useState("");
  const [editorText, setEditorText] = useState("");

  const { notice, showNotice, dismissNotice } = useNotice();
  const { busyAction, withBusy } = useBusyAction();

  const confirmResolverRef = useRef<((value: boolean) => void) | null>(null);

  const requestConfirm = useCallback((options: ConfirmModalOptions) => {
    return new Promise<boolean>((resolve) => {
      // 若外部逻辑同时触发多次确认弹窗，后者覆盖前者；前者默认视为取消。
      if (confirmResolverRef.current) {
        confirmResolverRef.current(false);
      }
      confirmResolverRef.current = resolve;
      setConfirmDialog(options);
    });
  }, []);

  const handleConfirmResult = useCallback((value: boolean) => {
    const resolve = confirmResolverRef.current;
    confirmResolverRef.current = null;
    setConfirmDialog(null);
    resolve?.(value);
  }, []);

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

  // ── 内部工具 ─────────────────────────────────────────

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

  // ── 窗口状态（标题栏自绘） ───────────────────────────

  useEffect(() => {
    void (async () => {
      try {
        const maximized = await getCurrentWindow().isMaximized();
        setWindowMaximized(maximized);
      } catch {
        // 在非 Tauri 环境（或权限受限）下忽略即可。
      }
    })();
  }, []);

  const handleMinimizeWindow = useCallback(async () => {
    try {
      await getCurrentWindow().minimize();
    } catch (error) {
      showNotice("error", `窗口最小化失败：${readableError(error)}`);
    }
  }, [showNotice]);

  const handleToggleMaximizeWindow = useCallback(async () => {
    try {
      const appWindow = getCurrentWindow();
      await appWindow.toggleMaximize();
      const maximized = await appWindow.isMaximized();
      setWindowMaximized(maximized);
    } catch (error) {
      showNotice("error", `窗口最大化切换失败：${readableError(error)}`);
    }
  }, [showNotice]);

  const handleCloseWindow = useCallback(async () => {
    try {
      await getCurrentWindow().close();
    } catch (error) {
      showNotice("error", `窗口关闭失败：${readableError(error)}`);
    }
  }, [showNotice]);

  // 无边框窗口缩放：用边缘热区触发 `startResizeDragging`
  const handleResizeWindow = useCallback(
    async (direction: ResizeDirection) => {
      try {
        await getCurrentWindow().startResizeDragging(direction);
      } catch (error) {
        showNotice("error", `窗口缩放失败：${readableError(error)}`);
      }
    },
    [showNotice]
  );

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

  const handleUpdateStringSetting = useCallback(
    (key: "baseUrl" | "apiKey" | "model" | "updateProxy", value: string) => {
      if (key !== "updateProxy") {
        setProviderStatus(null);
      }
      setSettings((current) => ({ ...current, [key]: value }));
    },
    []
  );

  const handleUpdateNumberSetting = useCallback(
    (key: "timeoutMs" | "temperature" | "maxConcurrency", value: string) => {
      const parsed =
        key === "timeoutMs" || key === "maxConcurrency"
          ? Number.parseInt(value, 10)
          : Number.parseFloat(value);

      if (!Number.isFinite(parsed)) {
        return;
      }

      setProviderStatus(null);
      setSettings((current) => ({
        ...current,
        [key]:
          key === "timeoutMs"
            ? Math.max(1_000, parsed)
            : key === "maxConcurrency"
              ? Math.max(1, Math.min(8, parsed))
              : Math.max(0, Math.min(2, parsed))
      }));
    },
    []
  );

  const handleUpdateChunkPreset = useCallback(
    (value: AppSettings["chunkPreset"]) => {
      setProviderStatus(null);
      setSettings((current) => ({ ...current, chunkPreset: value }));
    },
    []
  );

  const handleUpdateSegmentationMode = useCallback(
    (value: AppSettings["segmentationMode"]) => {
      setProviderStatus(null);
      setSettings((current) => ({ ...current, segmentationMode: value }));
    },
    []
  );

  const handleUpdateRewriteMode = useCallback(
    (value: AppSettings["rewriteMode"]) => {
      setProviderStatus(null);
      setSettings((current) => ({ ...current, rewriteMode: value }));
    },
    []
  );

  const handleUpdatePromptPresetId = useCallback(
    (value: AppSettings["promptPresetId"]) => {
      setSettings((current) => ({ ...current, promptPresetId: value }));
    },
    []
  );

  const handleUpsertCustomPrompt = useCallback((template: PromptTemplate) => {
    setSettings((current) => {
      const existingIndex = current.customPrompts.findIndex(
        (item) => item.id === template.id
      );
      const nextPrompts =
        existingIndex >= 0
          ? current.customPrompts.map((item) =>
              item.id === template.id ? template : item
            )
          : [...current.customPrompts, template];

      return { ...current, customPrompts: nextPrompts };
    });
  }, []);

  const handleDeleteCustomPrompt = useCallback((templateId: string) => {
    setSettings((current) => {
      const nextPrompts = current.customPrompts.filter(
        (item) => item.id !== templateId
      );
      const nextPresetId =
        current.promptPresetId === templateId ? "humanizer_zh" : current.promptPresetId;
      return { ...current, customPrompts: nextPrompts, promptPresetId: nextPresetId };
    });
  }, []);

  const handleSaveSettings = useCallback(async () => {
    try {
      const saved = await withBusy("save-settings", () =>
        saveSettings(settings)
      );
      setSettings(saved);
      showNotice("success", "配置已保存，后续打开的文档会沿用当前接口与模型。");
      if (isSettingsReady(saved)) {
        closeSettings();
      }
    } catch (error) {
      showNotice("error", `保存失败：${readableError(error)}`);
    }
  }, [settings, withBusy, showNotice, closeSettings]);

  const handleTestProvider = useCallback(async () => {
    try {
      const result = await withBusy("test-provider", () =>
        testProvider(settings)
      );
      setProviderStatus(result);
      showNotice(result.ok ? "success" : "warning", result.message);
    } catch (error) {
      setProviderStatus({ ok: false, message: readableError(error) });
      showNotice("error", `连接测试失败：${readableError(error)}`);
    }
  }, [settings, withBusy, showNotice]);

  const handleCheckUpdate = useCallback(async () => {
    try {
      if (import.meta.env.DEV) {
        showNotice(
          "warning",
          [
            "你正在通过开发模式启动（start-lessai.bat / tauri dev）。",
            "应用内更新只对“已安装的 Release 版本”生效，不会覆盖当前源码运行实例。",
            "想升级源码：请 git 拉取最新 tag/分支后重新运行；想升级安装版：请从开始菜单启动已安装的 LessAI 再检查更新。"
          ].join("\n"),
          { autoDismissMs: 12_000 }
        );
        return;
      }

      const currentVersion = await getVersion();
      const bundleType = await getBundleType();

      if (bundleType === BundleType.Deb || bundleType === BundleType.Rpm) {
        showNotice(
          "warning",
          `当前安装包类型（${bundleType}）不支持应用内更新，请前往 GitHub Releases 下载新版本。`
        );
        return;
      }

      await withBusy("check-update", async () => {
        showNotice("info", "正在检查更新…", { autoDismissMs: null });

        const rawProxy = settings.updateProxy.trim();
        const proxy =
          rawProxy && !rawProxy.includes("://") ? `http://${rawProxy}` : rawProxy || undefined;

        const update = await check({ timeout: 15_000, proxy });
        if (!update) {
          showNotice("success", `已是最新版本（${currentVersion}）。`);
          return;
        }

        // 发现更新后进入确认弹窗，先收起“检查中”提示，避免干扰阅读。
        dismissNotice();

        const messageParts = [
          `当前版本：${currentVersion}`,
          `发现新版本：${update.version}`,
          update.date ? `发布时间：${update.date}` : null,
          update.body?.trim() ? `更新内容：\n${update.body.trim()}` : null,
          "",
          "是否立即下载并安装？"
        ].filter((item): item is string => Boolean(item));

        const ok = await requestConfirm({
          title: "发现新版本",
          message: messageParts.join("\n"),
          okLabel: "立即更新",
          cancelLabel: "稍后"
        });

        if (!ok) {
          await update.close();
          return;
        }

        let contentLength: number | null = null;
        let downloadedBytes = 0;
        let lastNoticeAt = 0;

        const pushDownloadNotice = (force = false) => {
          const now = Date.now();
          if (!force && now - lastNoticeAt < 120) return;
          lastNoticeAt = now;

          const totalBytes = contentLength ?? 0;
          const hasTotal = totalBytes > 0;
          const percent = hasTotal
            ? Math.max(0, Math.min(100, Math.floor((downloadedBytes / totalBytes) * 100)))
            : null;

          const progressText = hasTotal
            ? `${percent}%（${formatBytes(downloadedBytes)} / ${formatBytes(totalBytes)}）`
            : `已下载 ${formatBytes(downloadedBytes)}`;

          showNotice("info", `正在下载更新… ${progressText}`, { autoDismissMs: null });
        };

        pushDownloadNotice(true);

        try {
          await update.downloadAndInstall((event) => {
            switch (event.event) {
              case "Started":
                contentLength = event.data.contentLength ?? null;
                downloadedBytes = 0;
                pushDownloadNotice(true);
                break;
              case "Progress":
                downloadedBytes += event.data.chunkLength;
                pushDownloadNotice(false);
                break;
              case "Finished":
                showNotice("info", "下载完成，正在安装更新…", { autoDismissMs: null });
                break;
              default:
                break;
            }
          });
        } finally {
          try {
            await update.close();
          } catch {
            // ignore
          }
        }

        // 注意：Windows 平台由于系统限制，安装程序执行时应用可能会直接退出。
        // 其他平台安装完成后可调用 relaunch() 自动重启。
        try {
          showNotice("success", "更新已安装，正在重启应用…", { autoDismissMs: null });
          await relaunch();
        } catch (error) {
          showNotice("warning", `更新已安装，请手动重启应用：${readableError(error)}`);
        }
      });
    } catch (error) {
      const message = readableError(error);

      if (
        message.includes("Could not fetch a valid release JSON") ||
        /valid release json/i.test(message)
      ) {
        showNotice(
          "error",
          [
            "检查更新失败：无法从更新源拿到有效响应（GitHub 返回非 2xx）。",
            `更新源：${UPDATE_MANIFEST_URL}`,
            "如果浏览器能打开但应用内失败：通常是网络/代理差异，可在设置里填写“更新代理”（例如 http://127.0.0.1:7890）后重试。",
            "如果浏览器打开需要登录或是 404：说明 Release 资源未公开或 latest.json 尚未生成/上传。",
            `原始错误：${message}`
          ].join("\n"),
          { autoDismissMs: 12_000 }
        );
        return;
      }

      showNotice(
        "error",
        `检查更新失败：${message}${
          /updater|pubkey|endpoint|permission/i.test(message)
            ? "\n（提示：需要在 tauri.conf.json 配置 updater.endpoints/pubkey，并在 capabilities 授权 updater:default；Release 构建需合并 tauri.updater.conf.json 以生成签名产物）"
            : ""
        }`
      );
    }
  }, [dismissNotice, requestConfirm, settings.updateProxy, showNotice, withBusy]);

  // ── Document handlers ────────────────────────────────

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
      showNotice(
        "warning",
        "当前文档正在执行自动任务，请先取消或等待完成后再打开其他文件。"
      );
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

      const opened = await withBusy("open-document", () =>
        openDocument(path)
      );
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
  }, [applySessionState, closeSettings, showNotice, withBusy]);

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
  }, [busyAction, showNotice]);

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
  }, [showNotice]);

  const handleExitEditor = useCallback(() => {
    if (stageRef.current !== "editor") return;
    if (editorDirtyRef.current) {
      showNotice("warning", "你有未保存的手动编辑，请先保存或放弃修改。");
      return;
    }
    setStage("workbench");
  }, [showNotice]);

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
        const updated = await withBusy(actionKey, () =>
          saveDocumentEdits(session.id, content)
        );

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
    [applySessionState, showNotice, withBusy]
  );

  // ── Rewrite handlers ─────────────────────────────────

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

      try {
        const updated = await withBusy(`start-${mode}`, () =>
          startRewrite(session.id, mode)
        );
        if (mode === "manual") {
          const suggestion = getLatestSuggestion(updated);
          const nextChunkIndex =
            suggestion?.chunkIndex ?? selectDefaultChunkIndex(updated);

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
      withBusy,
      showNotice,
      applySessionState,
      refreshSessionState
    ]
  );

  const handlePause = useCallback(async () => {
    const session = currentSessionRef.current;
    if (!session) return;
    try {
      const updated = await withBusy("pause-rewrite", () =>
        pauseRewrite(session.id)
      );
      applySessionState(updated, activeChunkIndexRef.current, {
        preferredSuggestionId: activeSuggestionIdRef.current
      });
      showNotice("warning", "自动任务已暂停，可继续或取消。");
    } catch (error) {
      showNotice("error", `暂停失败：${readableError(error)}`);
    }
  }, [withBusy, showNotice, applySessionState]);

  const handleResume = useCallback(async () => {
    const session = currentSessionRef.current;
    if (!session) return;
    try {
      const updated = await withBusy("resume-rewrite", () =>
        resumeRewrite(session.id)
      );
      applySessionState(updated, activeChunkIndexRef.current, {
        preferredSuggestionId: activeSuggestionIdRef.current
      });
      showNotice("info", "自动任务已继续。");
    } catch (error) {
      showNotice("error", `继续失败：${readableError(error)}`);
    }
  }, [withBusy, showNotice, applySessionState]);

  const handleCancel = useCallback(async () => {
    const session = currentSessionRef.current;
    if (!session) return;
    try {
      const updated = await withBusy("cancel-rewrite", () =>
        cancelRewrite(session.id)
      );
      applySessionState(updated, activeChunkIndexRef.current, {
        preferredSuggestionId: activeSuggestionIdRef.current
      });
      setLiveProgress(null);
      showNotice("warning", "自动任务已取消，已保留当前文档进度。");
    } catch (error) {
      showNotice("error", `取消失败：${readableError(error)}`);
    }
  }, [withBusy, showNotice, applySessionState]);

  // ── Suggestion handlers ──────────────────────────────

  const handleSelectChunk = useCallback((index: number) => {
    const session = currentSessionRef.current;
    setActiveChunkIndex(index);

    if (!session) {
      setActiveSuggestionId(null);
      return;
    }

    let latestForChunk: { id: string; sequence: number } | null = null;
    for (const suggestion of session.suggestions) {
      if (suggestion.chunkIndex !== index) continue;
      if (!latestForChunk || suggestion.sequence > latestForChunk.sequence) {
        latestForChunk = { id: suggestion.id, sequence: suggestion.sequence };
      }
    }

    if (latestForChunk) {
      setActiveSuggestionId(latestForChunk.id);
      return;
    }

    setActiveSuggestionId(null);
  }, []);

  const handleSelectSuggestion = useCallback((suggestionId: string) => {
    setActiveSuggestionId(suggestionId);
    setReviewView("diff");
  }, []);

  const handleApplySuggestion = useCallback(
    async (suggestionId: string) => {
      const session = currentSessionRef.current;
      if (!session) return;

      try {
        const updated = await withBusy(`apply-suggestion:${suggestionId}`, () =>
          applySuggestion(session.id, suggestionId)
        );
        const suggestion =
          updated.suggestions.find((item) => item.id === suggestionId) ??
          getLatestSuggestion(updated);
        const chunkIndex = suggestion?.chunkIndex ?? activeChunkIndexRef.current;

        applySessionState(updated, chunkIndex, {
          preferredSuggestionId: suggestionId
        });

        showNotice(
          "success",
          suggestion
            ? `已应用修改对 #${suggestion.sequence}。`
            : "已应用修改对。"
        );
      } catch (error) {
        showNotice("error", `应用失败：${readableError(error)}`);
      }
    },
    [withBusy, showNotice, applySessionState]
  );

  const handleDismissSuggestion = useCallback(
    async (suggestionId: string) => {
      const session = currentSessionRef.current;
      if (!session) return;

      try {
        const updated = await withBusy(`dismiss-suggestion:${suggestionId}`, () =>
          dismissSuggestion(session.id, suggestionId)
        );
        const suggestion =
          updated.suggestions.find((item) => item.id === suggestionId) ??
          getLatestSuggestion(updated);
        const chunkIndex = suggestion?.chunkIndex ?? activeChunkIndexRef.current;

        applySessionState(updated, chunkIndex, {
          preferredSuggestionId: suggestion?.id ?? null
        });

        showNotice("warning", "已取消应用 / 忽略该修改对。");
      } catch (error) {
        showNotice("error", `操作失败：${readableError(error)}`);
      }
    },
    [withBusy, showNotice, applySessionState]
  );

  const handleDeleteSuggestion = useCallback(
    async (suggestionId: string) => {
      const session = currentSessionRef.current;
      if (!session) return;

      const target = session.suggestions.find((item) => item.id === suggestionId);
      const targetChunkIndex = target?.chunkIndex ?? activeChunkIndexRef.current;

      try {
        const updated = await withBusy(`delete-suggestion:${suggestionId}`, () =>
          deleteSuggestion(session.id, suggestionId)
        );
        const nextChunkIndex = Math.min(
          targetChunkIndex,
          Math.max(0, updated.chunks.length - 1)
        );
        applySessionState(updated, nextChunkIndex);
        showNotice("warning", "已删除该修改对。");
      } catch (error) {
        showNotice("error", `删除失败：${readableError(error)}`);
      }
    },
    [withBusy, showNotice, applySessionState]
  );

  const handleRetry = useCallback(async () => {
    const session = currentSessionRef.current;
    const chunk = session?.chunks[activeChunkIndexRef.current];
    if (!session || !chunk) return;
    try {
      const updated = await withBusy("retry-chunk", () =>
        retryChunk(session.id, chunk.index)
      );
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
  }, [withBusy, showNotice, applySessionState, refreshSessionState]);

  // ── Export ────────────────────────────────────────────

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
      const savedPath = await withBusy("export-document", () =>
        exportDocument(session.id, path)
      );
      showNotice("success", `已导出到 ${formatDisplayPath(savedPath)}`);
    } catch (error) {
      showNotice("error", `导出失败：${readableError(error)}`);
    }
  }, [withBusy, showNotice]);

  // ── Finalize：覆盖原文件 + 清理记录 ───────────────────

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
      showNotice(
        "success",
        `已覆盖并清理，并重新打开：${savedPath ? formatDisplayPath(savedPath) : ""}`
      );
    } catch (error) {
      if (savedPath) {
        startTransition(() => {
          setCurrentSession(null);
          setActiveChunkIndex(0);
          setActiveSuggestionId(null);
          setReviewView("diff");
          setLiveProgress(null);
        });
        showNotice(
          "warning",
          `已覆盖并清理，但重新打开失败：${readableError(error)}`
        );
        return;
      }

      showNotice("error", `写回失败：${readableError(error)}`);
    }
  }, [applySessionState, closeSettings, showNotice, withBusy]);

  // ── Reset Session：清空记录 + 重新切块（不修改原文件） ─────

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
  }, [applySessionState, showNotice, withBusy]);

  // ── 渲染 ─────────────────────────────────────────────

  if (booting) {
    return (
      <div className="boot-screen">
        <div className="boot-card">
          <LoaderCircle className="spin" />
          <div>
            <p>LessAI</p>
            <strong>正在装载单屏工作台</strong>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="app-shell">
      <div className="body-shell">
        <main className="workspace">
          <div
            className="workspace-bar"
            data-tauri-drag-region
            onPointerDown={(event) => {
              if (event.button !== 0) return;
              const target = event.target as HTMLElement | null;
              if (target?.closest?.('[data-tauri-drag-region="false"]')) return;
              event.preventDefault();
              void getCurrentWindow().startDragging();
            }}
          >
            <div className="workspace-bar-left">
              <img className="brand-logo is-small" src={logoUrl} alt="LessAI" />
              <div className="workspace-bar-brand">
                <strong>LessAI</strong>
                <span className="workspace-bar-view">
                  {settingsOpen ? "Settings" : stage === "editor" ? "Editor" : "Workbench"}
                </span>
              </div>
            </div>

            <div className="workspace-bar-center">
              <strong className="workspace-bar-session">
                {currentSession ? currentSession.title : "未打开文档"}
              </strong>
              <div
                className="workspace-bar-chips scroll-region"
                data-tauri-drag-region="false"
              >
                <StatusBadge
                  tone={
                    currentSession
                      ? statusTone(currentSession.status)
                      : settingsReady
                        ? "info"
                        : "warning"
                  }
                >
                  {currentSession
                    ? formatSessionStatus(currentSession.status)
                    : settingsReady
                      ? "未打开"
                      : "未配置"}
                </StatusBadge>
                {currentSession ? (
                  <span className="context-chip">
                    路径：{formatDisplayPath(currentSession.documentPath)}
                  </span>
                ) : null}
                <span className="context-chip">模型：{settings.model}</span>
                <span className="context-chip">应用：{topbarProgress}</span>
                {liveProgress &&
                currentSession &&
                liveProgress.sessionId === currentSession.id ? (
                  <span className="context-chip">
                    进度 {liveProgress.completedChunks}/{liveProgress.totalChunks}
                    {liveProgress.inFlight > 0 ? ` · 进行中 ${liveProgress.inFlight}` : ""}
                    {liveProgress.maxConcurrency > 1
                      ? ` · 并发 ${liveProgress.maxConcurrency}`
                      : ""}
                  </span>
                ) : null}
              </div>
            </div>

            <div className="workspace-bar-actions" data-tauri-drag-region="false">
              <button
                type="button"
                className="icon-button"
                onClick={handleOpenDocument}
                aria-label="打开文档"
                title="打开文件"
                disabled={
                  stage === "editor" ||
                  Boolean(busyAction) ||
                  Boolean(
                    currentSession &&
                      ["running", "paused"].includes(currentSession.status)
                  )
                }
              >
                {busyAction === "open-document" ? (
                  <LoaderCircle className="spin" />
                ) : (
                  <FolderOpen />
                )}
              </button>

              <button
                type="button"
                className="icon-button"
                onClick={openSettings}
                aria-label="打开设置"
                title="设置"
              >
                <Settings2 />
              </button>
              <button
                type="button"
                className="icon-button"
                onClick={handleExport}
                aria-label="导出终稿"
                title="导出"
                disabled={
                  stage === "editor" ||
                  !currentSession ||
                  Boolean(busyAction) ||
                  Boolean(
                    currentSession &&
                      ["running", "paused"].includes(currentSession.status)
                  )
                }
              >
                {busyAction === "export-document" ? (
                  <LoaderCircle className="spin" />
                ) : (
                  <Download />
                )}
              </button>
            </div>

            <div className="window-controls" data-tauri-drag-region="false">
              <button
                type="button"
                className="window-control-button"
                onClick={() => void handleMinimizeWindow()}
                aria-label="最小化窗口"
                title="最小化"
              >
                <Minus />
              </button>
              <button
                type="button"
                className="window-control-button"
                onClick={() => void handleToggleMaximizeWindow()}
                aria-label={windowMaximized ? "还原窗口" : "最大化窗口"}
                title={windowMaximized ? "还原" : "最大化"}
              >
                {windowMaximized ? <Copy /> : <Square />}
              </button>
              <button
                type="button"
                className="window-control-button is-close"
                onClick={() => void handleCloseWindow()}
                aria-label="关闭窗口"
                title="关闭"
              >
                <X />
              </button>
            </div>
          </div>

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
              onCancel={() => void handleCancel()}
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

          {notice ? (
            <div className="toast-layer" aria-live="polite" aria-label="操作提示">
              <div className={`notice is-${notice.tone} toast`}>
                <span>{notice.message}</span>
                <button
                  type="button"
                  className="notice-dismiss"
                  onClick={dismissNotice}
                  aria-label="关闭提示"
                  title="关闭"
                >
                  <X />
                </button>
              </div>
            </div>
          ) : null}

          <SettingsModal
            open={settingsOpen}
            settings={settings}
            providerStatus={providerStatus}
            busyAction={busyAction}
            onClose={closeSettings}
            onUpdateStringSetting={handleUpdateStringSetting}
            onUpdateNumberSetting={handleUpdateNumberSetting}
            onUpdateChunkPreset={handleUpdateChunkPreset}
            onUpdateSegmentationMode={handleUpdateSegmentationMode}
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

      <div className="window-resize-layer" aria-hidden="true">
        <button
          type="button"
          className="resize-handle is-n"
          tabIndex={-1}
          onPointerDown={(event) => {
            if (event.button !== 0) return;
            event.preventDefault();
            void handleResizeWindow("North");
          }}
        />
        <button
          type="button"
          className="resize-handle is-e"
          tabIndex={-1}
          onPointerDown={(event) => {
            if (event.button !== 0) return;
            event.preventDefault();
            void handleResizeWindow("East");
          }}
        />
        <button
          type="button"
          className="resize-handle is-s"
          tabIndex={-1}
          onPointerDown={(event) => {
            if (event.button !== 0) return;
            event.preventDefault();
            void handleResizeWindow("South");
          }}
        />
        <button
          type="button"
          className="resize-handle is-w"
          tabIndex={-1}
          onPointerDown={(event) => {
            if (event.button !== 0) return;
            event.preventDefault();
            void handleResizeWindow("West");
          }}
        />

        <button
          type="button"
          className="resize-handle is-nw"
          tabIndex={-1}
          onPointerDown={(event) => {
            if (event.button !== 0) return;
            event.preventDefault();
            void handleResizeWindow("NorthWest");
          }}
        />
        <button
          type="button"
          className="resize-handle is-ne"
          tabIndex={-1}
          onPointerDown={(event) => {
            if (event.button !== 0) return;
            event.preventDefault();
            void handleResizeWindow("NorthEast");
          }}
        />
        <button
          type="button"
          className="resize-handle is-se"
          tabIndex={-1}
          onPointerDown={(event) => {
            if (event.button !== 0) return;
            event.preventDefault();
            void handleResizeWindow("SouthEast");
          }}
        />
        <button
          type="button"
          className="resize-handle is-sw"
          tabIndex={-1}
          onPointerDown={(event) => {
            if (event.button !== 0) return;
            event.preventDefault();
            void handleResizeWindow("SouthWest");
          }}
        />
      </div>
    </div>
  );
}
