import type {
  CapabilityGate,
  DocumentSession,
  DocumentSnapshot,
  RewriteUnit,
  RunningState,
  SlotUpdate,
  WritebackSlot
} from "./types";

export function mergedTextFromSlotIds(slots: WritebackSlot[], slotIds: string[]) {
  return slotIds
    .map((slotId) => {
      const slot = slots.find((item) => item.id === slotId);
      return slot ? `${slot.text}${slot.separatorAfter}` : "";
    })
    .join("");
}

export function rewriteUnitSourceText(session: DocumentSession, rewriteUnit: RewriteUnit) {
  return mergedTextFromSlotIds(session.writebackSlots, rewriteUnit.slotIds);
}

export function applySlotUpdates(slots: WritebackSlot[], updates: SlotUpdate[]) {
  const next = slots.map((slot) => ({ ...slot }));
  for (const update of updates) {
    const slot = next.find((item) => item.id === update.slotId);
    if (!slot) {
      throw new Error(`未知 slot_id：${update.slotId}`);
    }
    if (!slot.editable) {
      throw new Error(`locked slot 不允许修改：${update.slotId}`);
    }
    slot.text = update.text;
  }
  return next;
}

export function buildAppliedProjection(session: DocumentSession) {
  const applied = session.suggestions
    .filter((item) => item.decision === "applied")
    .sort((a, b) => a.sequence - b.sequence);
  let projected = session.writebackSlots.map((slot) => ({ ...slot }));
  for (const suggestion of applied) {
    projected = applySlotUpdates(projected, suggestion.slotUpdates);
  }
  return projected;
}

export function mergedTextFromSlots(slots: WritebackSlot[]) {
  return slots.map((slot) => `${slot.text}${slot.separatorAfter}`).join("");
}

export function findSuggestionIndex(session: DocumentSession, suggestionId: string) {
  return session.suggestions.findIndex((item) => item.id === suggestionId);
}

export function dismissAppliedSuggestionsForUnit(
  session: DocumentSession,
  rewriteUnitId: string,
  now: string
) {
  for (const suggestion of session.suggestions) {
    if (
      suggestion.rewriteUnitId === rewriteUnitId &&
      suggestion.decision === "applied"
    ) {
      suggestion.decision = "dismissed";
      suggestion.updatedAt = now;
    }
  }
}

export function applySuggestionById(session: DocumentSession, suggestionId: string, now: string) {
  const index = findSuggestionIndex(session, suggestionId);
  if (index < 0) {
    throw new Error("未找到对应的修改对。");
  }
  const rewriteUnitId = session.suggestions[index].rewriteUnitId;
  dismissAppliedSuggestionsForUnit(session, rewriteUnitId, now);
  session.suggestions[index].decision = "applied";
  session.suggestions[index].updatedAt = now;
  return rewriteUnitId;
}

export function computeSessionState(session: DocumentSession): RunningState {
  if (session.rewriteUnits.some((unit) => unit.status === "failed")) {
    return "failed";
  }
  if (session.rewriteUnits.every((unit) => unit.status === "done")) {
    return "completed";
  }
  return "idle";
}

export function clearRunningUnits(session: DocumentSession) {
  for (const unit of session.rewriteUnits) {
    if (unit.status === "running") {
      unit.status = "idle";
    }
  }
}

export function updateSessionTimestamp(
  session: DocumentSession,
  nowIso: () => string,
  hydrateCapabilities: (session: DocumentSession) => void
) {
  session.updatedAt = nowIso();
  hydrateCapabilities(session);
}

export function getSessionOrThrow(sessions: Map<string, DocumentSession>, sessionId: string) {
  const session = sessions.get(sessionId);
  if (!session) {
    throw new Error(`未找到会话：${sessionId}`);
  }
  return session;
}

export function ensureNoActiveJob(
  autoJobs: Map<string, { cancelled: boolean }>,
  sessionId: string,
  errorMessage: string
) {
  const job = autoJobs.get(sessionId);
  if (!job) {
    return;
  }
  if (!job.cancelled) {
    throw new Error(errorMessage);
  }
}

export function ensureCapabilityAllowed(gate: CapabilityGate, fallbackMessage: string) {
  if (gate.allowed) {
    return;
  }
  throw new Error(gate.blockReason ?? fallbackMessage);
}

export function ensureSessionCanUseEditorWriteback(session: DocumentSession) {
  ensureCapabilityAllowed(session.capabilities.sourceWriteback, "当前文档暂不支持写回原文件。");
  ensureCapabilityAllowed(session.capabilities.editorEntry, "当前文档暂不支持进入编辑模式。");
}

export function ensureSnapshotMatchesPath(
  path: string,
  expected: DocumentSnapshot | null | undefined,
  getVirtualFile: (path: string) => { text: string } | null,
  snapshotFromText: (text: string) => DocumentSnapshot,
  snapshotMissingError: string,
  snapshotMismatchError: string
) {
  if (!expected) {
    throw new Error(snapshotMissingError);
  }
  const file = getVirtualFile(path);
  if (!file) {
    throw new Error("网页缓存中未找到该 TXT 文件，请重新选择文件。");
  }
  const current = snapshotFromText(file.text);
  if (current.sha256 !== expected.sha256) {
    throw new Error(snapshotMismatchError);
  }
}

export function ensureSessionSourceMatches(
  session: DocumentSession,
  ensureSnapshot: (
    path: string,
    expected: DocumentSnapshot | null | undefined
  ) => void
) {
  ensureSnapshot(session.documentPath, session.sourceSnapshot ?? null);
}

export function ensureEditorBaseSnapshotMatches(
  session: DocumentSession,
  editorBaseSnapshot: DocumentSnapshot | null | undefined,
  ensureSnapshot: (
    path: string,
    expected: DocumentSnapshot | null | undefined
  ) => void,
  snapshotMissingError: string,
  snapshotMismatchError: string,
  editorBaseSnapshotMissingError: string,
  editorBaseSnapshotExpiredError: string
) {
  try {
    ensureSnapshot(session.documentPath, editorBaseSnapshot);
  } catch (error) {
    if (error instanceof Error) {
      if (error.message === snapshotMissingError) {
        throw new Error(editorBaseSnapshotMissingError);
      }
      if (error.message === snapshotMismatchError) {
        throw new Error(editorBaseSnapshotExpiredError);
      }
    }
    throw error;
  }
}

export function resolveTargetRewriteUnitIds(
  session: DocumentSession,
  targetRewriteUnitIds?: string[] | null
) {
  if (!targetRewriteUnitIds || targetRewriteUnitIds.length === 0) {
    return null;
  }
  const selected = new Set<string>();
  for (const unitId of targetRewriteUnitIds) {
    const unit = session.rewriteUnits.find((item) => item.id === unitId);
    if (!unit) {
      throw new Error(`所选改写单元不存在：${unitId}`);
    }
    if (unit.status === "done") {
      continue;
    }
    selected.add(unitId);
  }
  if (selected.size === 0) {
    throw new Error("所选改写单元均不可改写。");
  }
  return selected;
}

function isTargetUnit(target: Set<string> | null, unitId: string) {
  return target ? target.has(unitId) : true;
}

export function ensureTargetsAvailable<T>(
  targets: T,
  hasSubset: boolean,
  isEmpty: (value: T) => boolean
) {
  if (!isEmpty(targets)) {
    return targets;
  }
  if (hasSubset) {
    throw new Error("所选改写单元已处理完成。");
  }
  throw new Error("没有可继续处理的改写单元，当前文档可能已经全部完成。");
}

export function nextManualBatch(
  session: DocumentSession,
  target: Set<string> | null,
  batchSize: number
) {
  return session.rewriteUnits
    .filter(
      (unit) =>
        isTargetUnit(target, unit.id) &&
        (unit.status === "idle" || unit.status === "failed")
    )
    .slice(0, Math.max(1, batchSize))
    .map((unit) => unit.id);
}

export function autoPendingQueue(session: DocumentSession, target: Set<string> | null) {
  return session.rewriteUnits
    .filter((unit) => isTargetUnit(target, unit.id) && unit.status !== "done")
    .map((unit) => unit.id);
}

export function targetTotals(session: DocumentSession, target: Set<string> | null) {
  const units = session.rewriteUnits.filter((unit) => isTargetUnit(target, unit.id));
  return {
    total: units.length,
    completed: units.filter((unit) => unit.status === "done").length
  };
}

export function randomId(prefix: string) {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return `${prefix}-${crypto.randomUUID()}`;
  }
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 9)}`;
}

export function markUnitsRunning(
  session: DocumentSession,
  rewriteUnitIds: string[],
  updateSessionTimestamp: (session: DocumentSession) => void
) {
  for (const unitId of rewriteUnitIds) {
    const unit = session.rewriteUnits.find((item) => item.id === unitId);
    if (!unit) {
      throw new Error("改写单元不存在。");
    }
    unit.status = "running";
    unit.errorMessage = null;
  }
  if (session.status !== "paused") {
    session.status = "running";
  }
  updateSessionTimestamp(session);
}

export function markBatchFailure(
  session: DocumentSession,
  rewriteUnitIds: string[],
  error: string,
  clearRunningUnits: (session: DocumentSession) => void,
  updateSessionTimestamp: (session: DocumentSession) => void
) {
  for (const unitId of rewriteUnitIds) {
    const unit = session.rewriteUnits.find((item) => item.id === unitId);
    if (!unit) {
      throw new Error("改写单元不存在。");
    }
    unit.status = "failed";
    unit.errorMessage = error;
  }
  session.status = "failed";
  clearRunningUnits(session);
  updateSessionTimestamp(session);
}

export function saveFinalizeRecord(
  record: {
    sessionId: string;
    documentPath: string;
    title: string;
    beforeText: string;
    afterText: string;
  },
  ensureBrowserStorage: () => Storage,
  finalizeRecordsStorageKey: string,
  randomId: (prefix: string) => string,
  nowIso: () => string
) {
  try {
    const storage = ensureBrowserStorage();
    const raw = storage.getItem(finalizeRecordsStorageKey);
    const list = raw ? (JSON.parse(raw) as unknown[]) : [];
    const next = [
      {
        id: randomId("finalize"),
        createdAt: nowIso(),
        ...record
      },
      ...list
    ].slice(0, 30);
    storage.setItem(finalizeRecordsStorageKey, JSON.stringify(next));
  } catch (error) {
    console.warn("[lessai::web] finalize record save failed", error);
  }
}

export function downloadTextFile(filename: string, content: string) {
  const blob = new Blob([content], { type: "text/plain;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  link.click();
  URL.revokeObjectURL(url);
}
