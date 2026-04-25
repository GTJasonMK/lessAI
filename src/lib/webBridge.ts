import { DEFAULT_SETTINGS } from "./constants";
import { createAutoJobRuntime, type WebAutoJob } from "./webBridgeAutoJobRuntime";
import { buildEditorWritebackPayload } from "./webBridgeEditorWriteback";
import { createRewritePipeline } from "./webBridgeRewritePipeline";
import { createRewriteCommands } from "./webBridgeRewriteCommands";
import { createSessionCommands } from "./webBridgeSessionCommands";
import { createSessionLifecycle } from "./webBridgeSessionLifecycle";
import { createSettingsCommands } from "./webBridgeSettingsCommands";
import { snapshotFromText } from "./webBridgeText";
import { validateSettings } from "./webBridgeModelApi";
import {
  applySuggestionById,
  clearRunningUnits,
  computeSessionState,
  ensureCapabilityAllowed,
  ensureEditorBaseSnapshotMatches as ensureEditorBaseSnapshotMatchesUtil,
  ensureNoActiveJob as ensureNoActiveJobUtil,
  ensureSessionSourceMatches as ensureSessionSourceMatchesUtil,
  ensureSnapshotMatchesPath as ensureSnapshotMatchesPathUtil,
  findSuggestionIndex,
  getSessionOrThrow as getSessionOrThrowUtil,
  markBatchFailure as markBatchFailureUtil,
  markUnitsRunning as markUnitsRunningUtil,
  randomId,
  saveFinalizeRecord as saveFinalizeRecordUtil,
  updateSessionTimestamp as updateSessionTimestampUtil
} from "./webBridgeSessionUtils";
import { createWritebackCommands } from "./webBridgeWritebackCommands";
import type {
  AppSettings,
  DocumentSession,
  DocumentSnapshot,
  EditorSlotEdit,
  RewriteMode
} from "./types";
import { getVirtualFile, updateVirtualFileText } from "./webFileStore";

const SETTINGS_STORAGE_KEY = "lessai.web.settings.v1";
const FINALIZE_RECORDS_STORAGE_KEY = "lessai.web.finalize.records.v1";
const ACTIVE_REWRITE_SESSION_ERROR = "当前文档正在执行自动任务，请先暂停或取消。";
const ACTIVE_JOB_RESET_SESSION_ERROR = "当前文档正在执行自动任务，请先暂停并取消后再重置。";
const ACTIVE_JOB_FINALIZE_ERROR = "当前文档正在执行自动任务，请先暂停并取消后再写回原文件。";
const ACTIVE_EDITOR_SESSION_ERROR = "当前文档正在执行自动任务，请先暂停并取消后再继续编辑。";
const SNAPSHOT_MISSING_ERROR = "当前会话缺少原文件快照，无法确认写回安全性。请重新导入文档后再写回。";
const SNAPSHOT_MISMATCH_ERROR = "原文件已在外部发生变化。为避免误写，请重新导入。";
const AI_REWRITE_BLOCK_REASON = "当前文档暂不支持安全写回覆盖，因此不允许继续 AI 改写。";
const EDITOR_BASE_SNAPSHOT_MISSING_ERROR =
  "当前编辑器缺少打开时的文件快照，无法确认保存安全性。请重新进入编辑模式后再试。";
const EDITOR_BASE_SNAPSHOT_EXPIRED_ERROR =
  "编辑器基准已过期，原文件已在外部发生变化。请重新进入编辑模式后再试。";
const DIRTY_SESSION_BLOCK_REASON =
  "该文档存在修订记录或进度，为避免冲突，请先“覆写并清理记录”或“重置记录”后再编辑。";
const REWRITE_UNIT_RISK_WARNING_NON_WHITESPACE_CHARS = 6000;

const sessions = new Map<string, DocumentSession>();
const autoJobs = new Map<string, WebAutoJob>();

function deepClone<T>(value: T): T {
  if (typeof structuredClone === "function") {
    return structuredClone(value);
  }
  return JSON.parse(JSON.stringify(value)) as T;
}

function nowIso() {
  return new Date().toISOString();
}

function ensureBrowserStorage() {
  if (typeof window === "undefined") {
    throw new Error("当前运行环境不支持网页存储。");
  }
  return window.localStorage;
}

function loadStoredSettings(): AppSettings {
  try {
    const storage = ensureBrowserStorage();
    const raw = storage.getItem(SETTINGS_STORAGE_KEY);
    if (!raw) {
      return deepClone(DEFAULT_SETTINGS);
    }

    const parsed = JSON.parse(raw) as Partial<AppSettings>;
    const merged = {
      ...DEFAULT_SETTINGS,
      ...parsed,
      customPrompts: Array.isArray(parsed.customPrompts) ? parsed.customPrompts : []
    };
    return validateSettings(merged);
  } catch {
    return deepClone(DEFAULT_SETTINGS);
  }
}

let cachedSettings: AppSettings | null = null;

function getSettings() {
  if (!cachedSettings) {
    cachedSettings = loadStoredSettings();
  }
  return cachedSettings;
}

function persistSettings(settings: AppSettings) {
  const storage = ensureBrowserStorage();
  storage.setItem(SETTINGS_STORAGE_KEY, JSON.stringify(settings));
}

const sessionLifecycle = createSessionLifecycle({
  sessions,
  nowIso,
  getSettings,
  getVirtualFile,
  snapshotMismatchError: SNAPSHOT_MISMATCH_ERROR,
  aiRewriteBlockReason: AI_REWRITE_BLOCK_REASON,
  dirtySessionBlockReason: DIRTY_SESSION_BLOCK_REASON
});

const {
  hydrateCapabilities,
  extractFileTitle,
  sessionIdFromPath,
  buildCleanSession,
  loadSessionInternal
} = sessionLifecycle;

const updateSessionTimestamp = (session: DocumentSession) =>
  updateSessionTimestampUtil(session, nowIso, hydrateCapabilities);

const getSessionOrThrow = (sessionId: string) =>
  getSessionOrThrowUtil(sessions, sessionId);

const ensureNoActiveJob = (sessionId: string, errorMessage: string) =>
  ensureNoActiveJobUtil(autoJobs, sessionId, errorMessage);

const ensureSnapshotMatchesPath = (
  path: string,
  expected: DocumentSnapshot | null | undefined
) =>
  ensureSnapshotMatchesPathUtil(
    path,
    expected,
    getVirtualFile,
    snapshotFromText,
    SNAPSHOT_MISSING_ERROR,
    SNAPSHOT_MISMATCH_ERROR
  );

const ensureSessionSourceMatches = (session: DocumentSession) =>
  ensureSessionSourceMatchesUtil(session, ensureSnapshotMatchesPath);

const ensureEditorBaseSnapshotMatches = (
  session: DocumentSession,
  editorBaseSnapshot: DocumentSnapshot | null | undefined
) =>
  ensureEditorBaseSnapshotMatchesUtil(
    session,
    editorBaseSnapshot,
    ensureSnapshotMatchesPath,
    SNAPSHOT_MISSING_ERROR,
    SNAPSHOT_MISMATCH_ERROR,
    EDITOR_BASE_SNAPSHOT_MISSING_ERROR,
    EDITOR_BASE_SNAPSHOT_EXPIRED_ERROR
  );

const markUnitsRunning = (session: DocumentSession, rewriteUnitIds: string[]) =>
  markUnitsRunningUtil(session, rewriteUnitIds, updateSessionTimestamp);

const markBatchFailure = (
  session: DocumentSession,
  rewriteUnitIds: string[],
  error: string
) =>
  markBatchFailureUtil(
    session,
    rewriteUnitIds,
    error,
    clearRunningUnits,
    updateSessionTimestamp
  );

const saveFinalizeRecord = (record: {
  sessionId: string;
  documentPath: string;
  title: string;
  beforeText: string;
  afterText: string;
}) =>
  saveFinalizeRecordUtil(
    record,
    ensureBrowserStorage,
    FINALIZE_RECORDS_STORAGE_KEY,
    randomId,
    nowIso
  );

const rewritePipeline = createRewritePipeline({
  getSettings,
  ensureCapabilityAllowed,
  ensureSessionSourceMatches,
  aiRewriteBlockReason: AI_REWRITE_BLOCK_REASON,
  deepClone,
  updateSessionTimestamp,
  nowIso,
  rewriteUnitRiskWarningNonWhitespaceChars: REWRITE_UNIT_RISK_WARNING_NON_WHITESPACE_CHARS
});

const { ensureSessionCanRewrite, validateSessionWriteback, processRewriteBatch } = rewritePipeline;

const autoJobRuntime = createAutoJobRuntime({
  sessions,
  autoJobs,
  getSettings,
  processRewriteBatch,
  markUnitsRunning,
  markBatchFailure,
  clearRunningUnits,
  updateSessionTimestamp,
  computeSessionState
});

const { emitRewriteProgress, emitRewriteUnitCompleted, runAutoJobLoop } = autoJobRuntime;

const settingsCommands = createSettingsCommands({
  deepClone,
  getSettings,
  persistSettings,
  setCachedSettings: (settings) => {
    cachedSettings = settings;
  }
});

const sessionCommands = createSessionCommands({
  sessions,
  deepClone,
  nowIso,
  getSettings,
  extractFileTitle,
  sessionIdFromPath,
  buildCleanSession,
  getVirtualFile,
  loadSessionInternal,
  ensureNoActiveJob,
  activeJobResetSessionError: ACTIVE_JOB_RESET_SESSION_ERROR,
  getSessionOrThrow,
  applySuggestionById,
  validateSessionWriteback,
  updateSessionTimestamp,
  findSuggestionIndex
});

const rewriteCommands = createRewriteCommands({
  autoJobs,
  deepClone,
  getSettings,
  getSessionOrThrow,
  loadSessionInternal,
  ensureNoActiveJob,
  ensureSessionCanRewrite,
  ensureEditorBaseSnapshotMatches,
  ensureSessionSourceMatches,
  markUnitsRunning,
  markBatchFailure,
  updateSessionTimestamp,
  clearRunningUnits,
  processRewriteBatch,
  emitRewriteUnitCompleted,
  emitRewriteProgress,
  runAutoJobLoop,
  activeRewriteSessionError: ACTIVE_REWRITE_SESSION_ERROR,
  activeEditorSessionError: ACTIVE_EDITOR_SESSION_ERROR
});

const writebackCommands = createWritebackCommands({
  sessions,
  deepClone,
  getSettings,
  getSessionOrThrow,
  ensureNoActiveJob,
  ensureEditorBaseSnapshotMatches,
  ensureSessionSourceMatches,
  buildEditorWritebackPayload,
  updateVirtualFileText,
  buildCleanSession,
  saveFinalizeRecord,
  activeEditorSessionError: ACTIVE_EDITOR_SESSION_ERROR,
  activeJobFinalizeError: ACTIVE_JOB_FINALIZE_ERROR
});

async function isMainWindowMaximizedCommand() {
  return false;
}

async function noOpCommand() {
  return;
}

type CommandPayload = Record<string, unknown> | undefined;

export async function webInvoke<T>(command: string, payload?: CommandPayload) {
  switch (command) {
    case "load_settings":
      return (await settingsCommands.loadSettingsCommand()) as T;
    case "save_settings":
      return (await settingsCommands.saveSettingsCommand(payload?.settings as AppSettings)) as T;
    case "test_provider":
      return (await settingsCommands.testProviderCommand(payload?.settings as AppSettings)) as T;
    case "list_release_versions":
      return (await settingsCommands.listReleaseVersionsCommand()) as T;
    case "switch_release_version":
      return (await settingsCommands.switchReleaseVersionCommand()) as T;
    case "install_system_package_release":
      return (await settingsCommands.installSystemPackageReleaseCommand()) as T;
    case "open_document":
      return (await sessionCommands.openDocumentCommand(String(payload?.path ?? ""))) as T;
    case "load_session":
      return (await sessionCommands.loadSessionCommand(String(payload?.sessionId ?? ""))) as T;
    case "reset_session":
      return (await sessionCommands.resetSessionCommand(String(payload?.sessionId ?? ""))) as T;
    case "start_rewrite":
      return (await rewriteCommands.startRewriteCommand(
        String(payload?.sessionId ?? ""),
        payload?.mode as RewriteMode,
        (payload?.targetRewriteUnitIds as string[] | undefined) ?? undefined
      )) as T;
    case "pause_rewrite":
      return (await rewriteCommands.pauseRewriteCommand(String(payload?.sessionId ?? ""))) as T;
    case "resume_rewrite":
      return (await rewriteCommands.resumeRewriteCommand(String(payload?.sessionId ?? ""))) as T;
    case "cancel_rewrite":
      return (await rewriteCommands.cancelRewriteCommand(String(payload?.sessionId ?? ""))) as T;
    case "retry_rewrite_unit":
      return (await rewriteCommands.retryRewriteUnitCommand(
        String(payload?.sessionId ?? ""),
        String(payload?.rewriteUnitId ?? "")
      )) as T;
    case "apply_suggestion":
      return (await sessionCommands.applySuggestionCommand(
        String(payload?.sessionId ?? ""),
        String(payload?.suggestionId ?? "")
      )) as T;
    case "dismiss_suggestion":
      return (await sessionCommands.dismissSuggestionCommand(
        String(payload?.sessionId ?? ""),
        String(payload?.suggestionId ?? "")
      )) as T;
    case "delete_suggestion":
      return (await sessionCommands.deleteSuggestionCommand(
        String(payload?.sessionId ?? ""),
        String(payload?.suggestionId ?? "")
      )) as T;
    case "export_document":
      return (await writebackCommands.exportDocumentCommand(
        String(payload?.sessionId ?? ""),
        String(payload?.path ?? "")
      )) as T;
    case "finalize_document":
      return (await writebackCommands.finalizeDocumentCommand(String(payload?.sessionId ?? ""))) as T;
    case "run_document_writeback":
      return (await writebackCommands.runDocumentWritebackCommand({
        sessionId: String(payload?.sessionId ?? ""),
        mode: payload?.mode as "validate" | "write",
        editorBaseSnapshot: (payload?.editorBaseSnapshot as DocumentSnapshot | null | undefined) ?? null,
        input: payload?.input as
          | { kind: "text"; content: string }
          | { kind: "slotEdits"; edits: EditorSlotEdit[] }
      })) as T;
    case "rewrite_selection":
      return (await rewriteCommands.rewriteSelectionCommand(
        String(payload?.sessionId ?? ""),
        String(payload?.text ?? ""),
        (payload?.editorBaseSnapshot as DocumentSnapshot | null | undefined) ?? null
      )) as T;
    case "is_main_window_maximized":
      return (await isMainWindowMaximizedCommand()) as T;
    case "minimize_main_window":
    case "toggle_maximize_main_window":
    case "close_main_window":
    case "start_drag_main_window":
    case "start_resize_main_window":
      return (await noOpCommand()) as T;
    default:
      throw new Error(`网页运行时未实现命令：${command}`);
  }
}
