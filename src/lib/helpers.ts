import type {
  AppSettings,
  RewriteUnitStatus,
  DocumentSession,
  RewriteSuggestion,
  RewriteUnit,
  RunningState,
  SlotUpdate,
  WritebackSlot,
} from "./types";
import type { NoticeTone } from "./constants";

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

export function isSettingsReady(settings: AppSettings) {
  return (
    settings.baseUrl.trim().length > 0 &&
    settings.apiKey.trim().length > 0 &&
    settings.model.trim().length > 0
  );
}

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

export function countCharacters(text: string) {
  return text.replace(/\s+/g, "").length;
}

export function normalizeNewlines(text: string) {
  return text.replace(/\r\n/g, "\n").replace(/\r/g, "\n");
}

const zhDateFormatter = new Intl.DateTimeFormat("zh-CN", {
  month: "2-digit",
  day: "2-digit",
  hour: "2-digit",
  minute: "2-digit"
});

export function formatDate(value: string) {
  return zhDateFormatter.format(new Date(value));
}

export function buildWritebackSlotMap(slots: ReadonlyArray<WritebackSlot>) {
  return new Map(slots.map((slot) => [slot.id, slot] as const));
}

export function resolveRewriteUnitSlots(
  session: DocumentSession,
  rewriteUnit: RewriteUnit
) {
  const slotMap = buildWritebackSlotMap(session.writebackSlots);
  return rewriteUnit.slotIds
    .map((slotId) => slotMap.get(slotId))
    .filter((slot): slot is WritebackSlot => slot != null);
}

export function mergedTextFromSlots(slots: ReadonlyArray<WritebackSlot>) {
  return slots.map((slot) => `${slot.text}${slot.separatorAfter}`).join("");
}

export function applySlotUpdatesToSlots(
  slots: ReadonlyArray<WritebackSlot>,
  updates: ReadonlyArray<SlotUpdate>
) {
  if (updates.length === 0) {
    return [...slots];
  }

  const nextTexts = new Map(updates.map((update) => [update.slotId, update.text] as const));
  return slots.map((slot) => ({
    ...slot,
    text: nextTexts.get(slot.id) ?? slot.text
  }));
}

export function rewriteUnitSourceText(
  session: DocumentSession,
  rewriteUnit: RewriteUnit
) {
  return mergedTextFromSlots(resolveRewriteUnitSlots(session, rewriteUnit));
}

export function rewriteUnitSlotsWithSuggestion(
  session: DocumentSession,
  rewriteUnit: RewriteUnit,
  suggestion: RewriteSuggestion | null
) {
  const slots = resolveRewriteUnitSlots(session, rewriteUnit);
  if (!suggestion) {
    return slots;
  }
  return applySlotUpdatesToSlots(slots, suggestion.slotUpdates);
}

export function rewriteUnitHasEditableSlot(
  session: DocumentSession,
  rewriteUnit: RewriteUnit
) {
  return resolveRewriteUnitSlots(session, rewriteUnit).some((slot) => slot.editable);
}

export function findRewriteUnit(
  session: DocumentSession,
  rewriteUnitId: string | null | undefined
) {
  if (!rewriteUnitId) return null;
  return session.rewriteUnits.find((item) => item.id === rewriteUnitId) ?? null;
}

export function formatSuggestionDecision(decision: RewriteSuggestion["decision"]) {
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

export function suggestionTone(decision: RewriteSuggestion["decision"]): NoticeTone {
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

export function groupSuggestionsByRewriteUnit(
  suggestions: ReadonlyArray<RewriteSuggestion>
) {
  const map = new Map<string, RewriteSuggestion[]>();
  for (const suggestion of suggestions) {
    const list = map.get(suggestion.rewriteUnitId);
    if (list) {
      list.push(suggestion);
    } else {
      map.set(suggestion.rewriteUnitId, [suggestion]);
    }
  }

  for (const [rewriteUnitId, list] of map.entries()) {
    list.sort((left, right) => left.sequence - right.sequence);
    map.set(rewriteUnitId, list);
  }

  return map;
}

export interface RewriteUnitSuggestionSummary {
  total: number;
  latest: RewriteSuggestion | null;
  applied: RewriteSuggestion | null;
  proposed: RewriteSuggestion | null;
  dismissedCount: number;
}

export function summarizeRewriteUnitSuggestions(
  suggestions: ReadonlyArray<RewriteSuggestion>
): RewriteUnitSuggestionSummary {
  if (suggestions.length === 0) {
    return {
      total: 0,
      latest: null,
      applied: null,
      proposed: null,
      dismissedCount: 0
    };
  }

  let applied: RewriteSuggestion | null = null;
  let proposed: RewriteSuggestion | null = null;
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

export function formatRewriteUnitStatus(
  session: DocumentSession,
  rewriteUnit: RewriteUnit,
  unitSuggestions: ReadonlyArray<RewriteSuggestion>
) {
  if (rewriteUnit.status === "running") {
    return "生成中";
  }

  if (rewriteUnit.status === "failed") {
    return "失败";
  }

  if (!rewriteUnitHasEditableSlot(session, rewriteUnit)) {
    return "跳过";
  }

  const summary = summarizeRewriteUnitSuggestions(unitSuggestions);
  if (summary.applied) {
    return "已应用";
  }

  if (summary.proposed) {
    return "待审阅";
  }

  if (rewriteUnit.status === "done" && summary.total > 0) {
    return "保留原文";
  }

  return "待生成";
}

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
  unitsTouched: number;
  unitsApplied: number;
  unitsProposed: number;
}

export function getSessionStats(session: DocumentSession): SessionStats {
  let idle = 0;
  let running = 0;
  let done = 0;
  let failed = 0;

  for (const rewriteUnit of session.rewriteUnits) {
    if (rewriteUnit.status === "idle") idle += 1;
    if (rewriteUnit.status === "running") running += 1;
    if (rewriteUnit.status === "done") done += 1;
    if (rewriteUnit.status === "failed") failed += 1;
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

  const suggestionsByRewriteUnit = groupSuggestionsByRewriteUnit(session.suggestions);
  let unitsTouched = 0;
  let unitsApplied = 0;
  let unitsProposed = 0;
  for (const list of suggestionsByRewriteUnit.values()) {
    if (list.length === 0) continue;
    unitsTouched += 1;
    const summary = summarizeRewriteUnitSuggestions(list);
    if (summary.applied) unitsApplied += 1;
    if (summary.proposed) unitsProposed += 1;
  }

  return {
    total: session.rewriteUnits.length,
    idle,
    running,
    done,
    failed,
    pendingGeneration: idle + failed,
    suggestionsTotal,
    suggestionsProposed,
    suggestionsApplied,
    suggestionsDismissed,
    unitsTouched,
    unitsApplied,
    unitsProposed
  };
}

function firstRewriteUnitIdBy(
  session: DocumentSession,
  predicate: (rewriteUnit: RewriteUnit) => boolean
) {
  return session.rewriteUnits.find((rewriteUnit) => predicate(rewriteUnit))?.id ?? null;
}

export function selectDefaultRewriteUnitId(session: DocumentSession) {
  const latest = getLatestSuggestion(session);
  if (latest) {
    return latest.rewriteUnitId;
  }

  const failedId = firstRewriteUnitIdBy(session, (rewriteUnit) => rewriteUnit.status === "failed");
  if (failedId) {
    return failedId;
  }

  const runningId = firstRewriteUnitIdBy(session, (rewriteUnit) => rewriteUnit.status === "running");
  if (runningId) {
    return runningId;
  }

  const idleId = firstRewriteUnitIdBy(session, (rewriteUnit) => rewriteUnit.status === "idle");
  if (idleId) {
    return idleId;
  }

  return session.rewriteUnits[0]?.id ?? null;
}

export function formatDisplayPath(path: string) {
  const value = path.trim();
  if (!value) return path;

  if (value.startsWith("\\\\?\\UNC\\")) {
    return `\\\\${value.slice("\\\\?\\UNC\\".length)}`;
  }

  if (value.startsWith("\\\\?\\")) {
    return value.slice("\\\\?\\".length);
  }

  if (value.startsWith("//?/UNC/")) {
    return `//${value.slice("//?/UNC/".length)}`;
  }

  if (value.startsWith("//?/")) {
    return value.slice("//?/".length);
  }

  return value;
}

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

export function rewriteBlockedReason(session: DocumentSession | null) {
  if (!session) return null;
  if (isPdfPath(session.documentPath)) return null;
  if (session.writeBackSupported) return null;
  return (
    session.writeBackBlockReason ??
    "当前文档暂不支持安全写回覆盖，因此不允许继续 AI 改写。"
  );
}

export function canRewriteSession(session: DocumentSession | null) {
  return rewriteBlockedReason(session) == null;
}

export function sanitizeFileName(name: string) {
  const cleaned = name.trim().replace(/[<>:"/\\|?*\u0000-\u001f]/g, "_");
  return cleaned.length > 0 ? cleaned : "lessai-result";
}

export function rewriteUnitStatusTone(
  session: DocumentSession,
  rewriteUnit: RewriteUnit,
  unitSuggestions: ReadonlyArray<RewriteSuggestion>
): NoticeTone {
  if (rewriteUnit.status === "failed") return "error";
  if (rewriteUnit.status === "running") return "info";
  if (!rewriteUnitHasEditableSlot(session, rewriteUnit)) return "info";

  const summary = summarizeRewriteUnitSuggestions(unitSuggestions);
  if (summary.applied) return "success";
  if (summary.proposed) return "warning";
  return "info";
}

export function isRewriteUnitDone(rewriteUnit: RewriteUnit) {
  return rewriteUnit.status === "done";
}

export function isRewriteUnitPending(rewriteUnit: RewriteUnit) {
  return rewriteUnit.status === "idle" || rewriteUnit.status === "failed";
}

export function hasEditableRewriteUnits(session: DocumentSession) {
  return session.rewriteUnits.some((rewriteUnit) => rewriteUnit.status !== "done");
}

export function buildRunningRewriteUnitIdSet(
  session: DocumentSession | null,
  liveProgress: { sessionId: string; runningUnitIds: string[] } | null
) {
  if (!session || !liveProgress || liveProgress.sessionId !== session.id) {
    return new Set<string>();
  }
  return new Set(liveProgress.runningUnitIds);
}

export function sortRewriteUnitsByOrder(units: ReadonlyArray<RewriteUnit>) {
  return [...units].sort((left, right) => left.order - right.order);
}
