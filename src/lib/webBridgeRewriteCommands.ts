import { callChatModel, ensureSettingsReady } from "./webBridgeModelApi";
import {
  WEB_DOCUMENT_FORMAT,
  buildRewriteUnitRequestFromSlots,
  parseRewriteUnitResponse,
  rewriteUnitSystemPrompt,
  rewriteUnitUserPrompt
} from "./webBridgeProtocol";
import {
  applySlotUpdates,
  autoPendingQueue,
  ensureSessionCanUseEditorWriteback,
  ensureTargetsAvailable,
  mergedTextFromSlots,
  nextManualBatch,
  resolveTargetRewriteUnitIds,
  targetTotals
} from "./webBridgeSessionUtils";
import { buildBoundaryAwareSlots, finalizePlainSelectionCandidate } from "./webBridgeText";
import type { CompletedRewriteUnitPayload, WebAutoJob } from "./webBridgeAutoJobRuntime";
import type {
  AppSettings,
  DocumentSession,
  DocumentSnapshot,
  RewriteMode,
  RunningState,
  SlotUpdate
} from "./types";

interface RewriteCommandDeps {
  autoJobs: Map<string, WebAutoJob>;
  deepClone: <T>(value: T) => T;
  getSettings: () => AppSettings;
  getSessionOrThrow: (sessionId: string) => DocumentSession;
  loadSessionInternal: (sessionId: string) => Promise<DocumentSession>;
  ensureNoActiveJob: (sessionId: string, errorMessage: string) => void;
  ensureSessionCanRewrite: (session: DocumentSession) => void;
  ensureEditorBaseSnapshotMatches: (
    session: DocumentSession,
    editorBaseSnapshot: DocumentSnapshot | null | undefined
  ) => void;
  ensureSessionSourceMatches: (session: DocumentSession) => void;
  markUnitsRunning: (session: DocumentSession, rewriteUnitIds: string[]) => void;
  markBatchFailure: (
    session: DocumentSession,
    rewriteUnitIds: string[],
    error: string
  ) => void;
  updateSessionTimestamp: (session: DocumentSession) => void;
  clearRunningUnits: (session: DocumentSession) => void;
  processRewriteBatch: (params: {
    session: DocumentSession;
    rewriteUnitIds: string[];
    autoApprove: boolean;
    signal?: AbortSignal;
  }) => Promise<CompletedRewriteUnitPayload[]>;
  emitRewriteUnitCompleted: (
    sessionId: string,
    completed: CompletedRewriteUnitPayload[]
  ) => Promise<void>;
  emitRewriteProgress: (
    session: DocumentSession,
    job: WebAutoJob,
    runningState: RunningState
  ) => Promise<void>;
  runAutoJobLoop: (sessionId: string) => Promise<void>;
  activeRewriteSessionError: string;
  activeEditorSessionError: string;
}

export function createRewriteCommands(deps: RewriteCommandDeps) {
  async function startRewriteCommand(
    sessionId: string,
    mode: RewriteMode,
    targetRewriteUnitIds?: string[]
  ) {
    deps.ensureNoActiveJob(sessionId, deps.activeRewriteSessionError);
    const session = await deps.loadSessionInternal(sessionId);
    deps.ensureSessionCanRewrite(session);
    ensureSettingsReady(deps.getSettings());

    const targetUnitIds = resolveTargetRewriteUnitIds(session, targetRewriteUnitIds);
    const hasTargetSubset = Boolean(targetUnitIds);

    if (mode === "manual") {
      const batch = ensureTargetsAvailable(
        nextManualBatch(session, targetUnitIds, deps.getSettings().unitsPerBatch),
        hasTargetSubset,
        (value) => value.length === 0
      );
      deps.markUnitsRunning(session, batch);
      try {
        const completed = await deps.processRewriteBatch({
          session,
          rewriteUnitIds: batch,
          autoApprove: false
        });
        session.status = "idle";
        deps.updateSessionTimestamp(session);
        await deps.emitRewriteUnitCompleted(sessionId, completed);
        return deps.deepClone(session);
      } catch (error) {
        deps.markBatchFailure(
          session,
          batch,
          error instanceof Error ? error.message : String(error)
        );
        throw error;
      }
    }

    const queue = ensureTargetsAvailable(
      autoPendingQueue(session, targetUnitIds),
      hasTargetSubset,
      (value) => value.length === 0
    );
    const totals = targetTotals(session, targetUnitIds);
    const job: WebAutoJob = {
      sessionId,
      queue: [...queue],
      targetUnitIds,
      completedUnits: totals.completed,
      totalUnits: totals.total,
      paused: false,
      cancelled: false,
      nextBatchToken: 1,
      controllers: new Map()
    };
    deps.autoJobs.set(sessionId, job);
    session.status = "running";
    deps.updateSessionTimestamp(session);
    void deps.emitRewriteProgress(session, job, "running");
    void deps.runAutoJobLoop(sessionId);
    return deps.deepClone(session);
  }

  async function pauseRewriteCommand(sessionId: string) {
    const session = deps.getSessionOrThrow(sessionId);
    const job = deps.autoJobs.get(sessionId);
    if (!job) {
      throw new Error("当前没有可暂停的任务。");
    }
    job.paused = true;
    session.status = "paused";
    deps.updateSessionTimestamp(session);
    await deps.emitRewriteProgress(session, job, "paused");
    return deps.deepClone(session);
  }

  async function resumeRewriteCommand(sessionId: string) {
    const session = deps.getSessionOrThrow(sessionId);
    const job = deps.autoJobs.get(sessionId);
    if (!job) {
      throw new Error("当前没有可继续的任务。");
    }
    job.paused = false;
    session.status = "running";
    deps.updateSessionTimestamp(session);
    await deps.emitRewriteProgress(session, job, "running");
    return deps.deepClone(session);
  }

  async function cancelRewriteCommand(sessionId: string) {
    const session = deps.getSessionOrThrow(sessionId);
    const job = deps.autoJobs.get(sessionId);
    if (job) {
      job.cancelled = true;
      for (const controller of job.controllers.values()) {
        controller.abort();
      }
    }
    session.status = "cancelled";
    deps.clearRunningUnits(session);
    deps.updateSessionTimestamp(session);
    return deps.deepClone(session);
  }

  async function retryRewriteUnitCommand(sessionId: string, rewriteUnitId: string) {
    deps.ensureNoActiveJob(sessionId, deps.activeRewriteSessionError);
    const session = await deps.loadSessionInternal(sessionId);
    deps.ensureSessionCanRewrite(session);
    const unit = session.rewriteUnits.find((item) => item.id === rewriteUnitId);
    if (!unit) {
      throw new Error("改写单元不存在。");
    }
    deps.markUnitsRunning(session, [rewriteUnitId]);
    try {
      const completed = await deps.processRewriteBatch({
        session,
        rewriteUnitIds: [rewriteUnitId],
        autoApprove: false
      });
      session.status = "idle";
      deps.updateSessionTimestamp(session);
      await deps.emitRewriteUnitCompleted(sessionId, completed);
      return deps.deepClone(session);
    } catch (error) {
      deps.markBatchFailure(
        session,
        [rewriteUnitId],
        error instanceof Error ? error.message : String(error)
      );
      throw error;
    }
  }

  async function rewriteSelectionCommand(
    sessionId: string,
    text: string,
    editorBaseSnapshot: DocumentSnapshot | null | undefined
  ) {
    if (!text.trim()) {
      throw new Error("选区内容为空。");
    }
    const session = deps.getSessionOrThrow(sessionId);
    deps.ensureNoActiveJob(sessionId, deps.activeEditorSessionError);
    deps.ensureEditorBaseSnapshotMatches(session, editorBaseSnapshot);
    deps.ensureSessionSourceMatches(session);
    ensureSessionCanUseEditorWriteback(session);
    const settings = deps.getSettings();
    ensureSettingsReady(settings);
    const slots = buildBoundaryAwareSlots(text);
    if (!slots.some((slot) => slot.editable && slot.text.trim())) {
      throw new Error("选区不包含可改写文本。");
    }

    const request = buildRewriteUnitRequestFromSlots(
      "selection",
      slots,
      WEB_DOCUMENT_FORMAT
    );
    const raw = await callChatModel(
      settings,
      rewriteUnitSystemPrompt(),
      rewriteUnitUserPrompt(request)
    );
    const response = parseRewriteUnitResponse(request, raw);
    const updates: SlotUpdate[] = response.updates.map((update) => {
      const sourceSlot = slots.find((slot) => slot.id === update.slotId);
      if (!sourceSlot) {
        throw new Error(`未知 slot_id：${update.slotId}。`);
      }
      const normalized = finalizePlainSelectionCandidate(sourceSlot.text, update.text);
      return {
        slotId: update.slotId,
        text: normalized
      };
    });
    const updatedSlots = applySlotUpdates(slots, updates);
    return mergedTextFromSlots(updatedSlots);
  }

  return {
    startRewriteCommand,
    pauseRewriteCommand,
    resumeRewriteCommand,
    cancelRewriteCommand,
    retryRewriteUnitCommand,
    rewriteSelectionCommand
  };
}
