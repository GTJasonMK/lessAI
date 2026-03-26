import type {
  AppSettings,
  EditSuggestion,
  ChunkTask,
  DocumentSession,
  RunningState
} from "./types";
import type { NoticeTone } from "./constants";

// ── 错误处理 ─────────────────────────────────────────────

export function readableError(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }

  if (typeof error === "string") {
    return error;
  }

  if (typeof error === "object" && error) {
    const maybeMessage = (error as { message?: unknown }).message;
    if (typeof maybeMessage === "string" && maybeMessage.trim().length > 0) {
      return maybeMessage;
    }

    const maybeError = (error as { error?: unknown }).error;
    if (typeof maybeError === "string" && maybeError.trim().length > 0) {
      return maybeError;
    }

    try {
      const json = JSON.stringify(error);
      if (json && json !== "{}") return json;
    } catch {
      // ignore
    }

    const asString = String(error);
    if (asString && asString !== "[object Object]") {
      return asString;
    }
  }

  return "发生了未识别的异常。";
}

// ── 验证 ─────────────────────────────────────────────────

export function isSettingsReady(settings: AppSettings) {
  return (
    settings.baseUrl.trim().length > 0 &&
    settings.apiKey.trim().length > 0 &&
    settings.model.trim().length > 0
  );
}

// ── 格式化 ───────────────────────────────────────────────

export function formatSessionStatus(status: RunningState) {
  switch (status) {
    case "idle":
      return "待机";
    case "running":
      return "执行中";
    case "paused":
      return "已暂停";
    case "completed":
      return "已完成";
    case "cancelled":
      return "已取消";
    case "failed":
      return "失败";
    default:
      return status;
  }
}

export function statusTone(status: RunningState): NoticeTone {
  switch (status) {
    case "completed":
      return "success";
    case "failed":
      return "error";
    case "paused":
    case "cancelled":
      return "warning";
    default:
      return "info";
  }
}

export function formatBytes(bytes: number) {
  if (!Number.isFinite(bytes) || bytes <= 0) {
    return "0 B";
  }

  const units = ["B", "KB", "MB", "GB", "TB"] as const;
  let value = bytes;
  let index = 0;

  while (value >= 1024 && index < units.length - 1) {
    value /= 1024;
    index += 1;
  }

  const fractionDigits = value >= 100 || index === 0 ? 0 : value >= 10 ? 1 : 2;
  return `${value.toFixed(fractionDigits)} ${units[index]}`;
}

// ── 文本统计 ─────────────────────────────────────────────

export function countCharacters(text: string) {
  return text.replace(/\s+/g, "").length;
}

export function normalizeNewlines(text: string) {
  return text.replace(/\r\n/g, "\n").replace(/\r/g, "\n");
}

// ── 日期格式化（缓存 Intl.DateTimeFormat 实例） ──────────

const zhDateFormatter = new Intl.DateTimeFormat("zh-CN", {
  month: "2-digit",
  day: "2-digit",
  hour: "2-digit",
  minute: "2-digit"
});

export function formatDate(value: string) {
  return zhDateFormatter.format(new Date(value));
}

// ── Suggestion 聚合 ──────────────────────────────────────

export function formatSuggestionDecision(decision: EditSuggestion["decision"]) {
  switch (decision) {
    case "proposed":
      return "待审阅";
    case "applied":
      return "已应用";
    case "dismissed":
      return "已忽略";
    default:
      return decision;
  }
}

export function suggestionTone(decision: EditSuggestion["decision"]): NoticeTone {
  switch (decision) {
    case "applied":
      return "success";
    case "proposed":
      return "warning";
    case "dismissed":
      return "info";
    default:
      return "info";
  }
}

export function groupSuggestionsByChunk(
  suggestions: ReadonlyArray<EditSuggestion>
) {
  const map = new Map<number, EditSuggestion[]>();
  for (const suggestion of suggestions) {
    const list = map.get(suggestion.chunkIndex);
    if (list) {
      list.push(suggestion);
    } else {
      map.set(suggestion.chunkIndex, [suggestion]);
    }
  }

  for (const [chunkIndex, list] of map.entries()) {
    list.sort((a, b) => a.sequence - b.sequence);
    map.set(chunkIndex, list);
  }

  return map;
}

export interface ChunkSuggestionSummary {
  total: number;
  latest: EditSuggestion | null;
  applied: EditSuggestion | null;
  proposed: EditSuggestion | null;
  dismissedCount: number;
}

export function summarizeChunkSuggestions(
  suggestions: ReadonlyArray<EditSuggestion>
): ChunkSuggestionSummary {
  if (suggestions.length === 0) {
    return {
      total: 0,
      latest: null,
      applied: null,
      proposed: null,
      dismissedCount: 0
    };
  }

  let applied: EditSuggestion | null = null;
  let proposed: EditSuggestion | null = null;
  let dismissedCount = 0;

  for (let index = suggestions.length - 1; index >= 0; index -= 1) {
    const suggestion = suggestions[index];
    if (suggestion.decision === "dismissed") {
      dismissedCount += 1;
    }
    if (!applied && suggestion.decision === "applied") {
      applied = suggestion;
    }
    if (!proposed && suggestion.decision === "proposed") {
      proposed = suggestion;
    }
    if (applied && proposed) {
      break;
    }
  }

  return {
    total: suggestions.length,
    latest: suggestions[suggestions.length - 1] ?? null,
    applied,
    proposed,
    dismissedCount
  };
}

export function getLatestSuggestion(session: DocumentSession) {
  if (session.suggestions.length === 0) {
    return null;
  }

  return session.suggestions.reduce((latest, current) =>
    current.sequence > latest.sequence ? current : latest
  );
}

export function formatChunkStatus(
  chunk: ChunkTask,
  chunkSuggestions: ReadonlyArray<EditSuggestion>
) {
  if (chunk.status === "running") {
    return "生成中";
  }

  if (chunk.status === "failed") {
    return "失败";
  }

  if (chunk.skipRewrite) {
    return "跳过";
  }

  const summary = summarizeChunkSuggestions(chunkSuggestions);
  if (summary.applied) {
    return "已应用";
  }

  if (summary.proposed) {
    return "待审阅";
  }

  if (chunk.status === "done" && summary.total > 0) {
    return "保留原文";
  }

  return "待生成";
}

// ── Session 统计 ─────────────────────────────────────────

export interface SessionStats {
  total: number;
  idle: number;
  running: number;
  done: number;
  failed: number;
  pendingGeneration: number;
  suggestionsTotal: number;
  suggestionsProposed: number;
  suggestionsApplied: number;
  suggestionsDismissed: number;
  chunksTouched: number;
  chunksApplied: number;
  chunksProposed: number;
}

export function getSessionStats(session: DocumentSession): SessionStats {
  let idle = 0;
  let running = 0;
  let done = 0;
  let failed = 0;

  for (const chunk of session.chunks) {
    if (chunk.status === "idle") idle += 1;
    if (chunk.status === "running") running += 1;
    if (chunk.status === "done") done += 1;
    if (chunk.status === "failed") failed += 1;
  }

  const suggestionsTotal = session.suggestions.length;
  let suggestionsProposed = 0;
  let suggestionsApplied = 0;
  let suggestionsDismissed = 0;
  for (const suggestion of session.suggestions) {
    if (suggestion.decision === "proposed") suggestionsProposed += 1;
    if (suggestion.decision === "applied") suggestionsApplied += 1;
    if (suggestion.decision === "dismissed") suggestionsDismissed += 1;
  }

  const suggestionsByChunk = groupSuggestionsByChunk(session.suggestions);
  let chunksTouched = 0;
  let chunksApplied = 0;
  let chunksProposed = 0;
  for (const list of suggestionsByChunk.values()) {
    if (list.length === 0) continue;
    chunksTouched += 1;
    const summary = summarizeChunkSuggestions(list);
    if (summary.applied) chunksApplied += 1;
    if (summary.proposed) chunksProposed += 1;
  }

  const total = session.chunks.length;
  const pendingGeneration = idle + failed;

  return {
    total,
    idle,
    running,
    done,
    failed,
    pendingGeneration,
    suggestionsTotal,
    suggestionsProposed,
    suggestionsApplied,
    suggestionsDismissed,
    chunksTouched,
    chunksApplied,
    chunksProposed
  };
}

// ── Chunk 查询 ───────────────────────────────────────────

function firstChunkIndexBy(
  session: DocumentSession,
  predicate: (chunk: ChunkTask) => boolean
) {
  const index = session.chunks.findIndex(predicate);
  return index >= 0 ? index : null;
}

export function selectDefaultChunkIndex(session: DocumentSession) {
  const latest = getLatestSuggestion(session);
  if (latest) {
    return latest.chunkIndex;
  }

  const failedIdx = firstChunkIndexBy(session, (chunk) => chunk.status === "failed");
  if (failedIdx != null) {
    return failedIdx;
  }

  const runningIdx = firstChunkIndexBy(session, (chunk) => chunk.status === "running");
  if (runningIdx != null) {
    return runningIdx;
  }

  const idleIdx = firstChunkIndexBy(session, (chunk) => chunk.status === "idle");
  if (idleIdx != null) {
    return idleIdx;
  }

  return 0;
}

// ── 路径显示（Windows `\\?\` 前缀清理） ─────────────────

export function formatDisplayPath(path: string) {
  const value = path.trim();
  if (!value) return path;

  // Windows 扩展路径前缀：
  // - `\\?\C:\...`
  // - `\\?\UNC\server\share\...`
  // 这些前缀对文件 IO 有用，但对 UI 展示非常“怪”。
  // 这里仅用于显示，不改变后端真实读写路径。
  if (value.startsWith("\\\\?\\UNC\\")) {
    return `\\\\${value.slice("\\\\?\\UNC\\".length)}`;
  }

  if (value.startsWith("\\\\?\\")) {
    return value.slice("\\\\?\\".length);
  }

  // 少数情况下可能是正斜杠版本（例如 `//?/C:/...`）
  if (value.startsWith("//?/UNC/")) {
    return `//${value.slice("//?/UNC/".length)}`;
  }

  if (value.startsWith("//?/")) {
    return value.slice("//?/".length);
  }

  return value;
}

// ── 扩展名判断（用于能力开关） ──────────────────────────

export function fileExtensionLower(path: string) {
  const value = path.trim();
  if (!value) return "";

  const lastSlash = Math.max(value.lastIndexOf("/"), value.lastIndexOf("\\"));
  const base = lastSlash >= 0 ? value.slice(lastSlash + 1) : value;
  const dot = base.lastIndexOf(".");
  if (dot <= 0) return "";
  return base.slice(dot + 1).toLowerCase();
}

export function isDocxPath(path: string) {
  return fileExtensionLower(path) === "docx";
}

export function isPdfPath(path: string) {
  return fileExtensionLower(path) === "pdf";
}

// ── 文件名清理 ───────────────────────────────────────────

export function sanitizeFileName(name: string) {
  const cleaned = name.trim().replace(/[<>:"/\\|?*\u0000-\u001f]/g, "_");
  return cleaned.length > 0 ? cleaned : "lessai-result";
}

// ── Chunk 状态对应 StatusBadge 色调 ──────────────────────

export function chunkStatusTone(
  chunk: ChunkTask,
  chunkSuggestions: ReadonlyArray<EditSuggestion>
): NoticeTone {
  if (chunk.status === "failed") return "error";
  if (chunk.status === "running") return "info";
  if (chunk.skipRewrite) return "info";

  const summary = summarizeChunkSuggestions(chunkSuggestions);
  if (summary.applied) return "success";
  if (summary.proposed) return "warning";
  return "info";
}
