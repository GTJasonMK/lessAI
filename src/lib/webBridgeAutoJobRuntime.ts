import {
  TAURI_EVENTS,
  type RewriteFailedPayload,
  type RewriteUnitCompletedPayload,
  type SessionEventPayload
} from "./constants";
import { emitRuntimeEvent } from "./runtimeEvents";
import type { DocumentSession, RewriteProgress, RunningState } from "./types";

export interface CompletedRewriteUnitPayload {
  rewriteUnitId: string;
  suggestionId: string;
  suggestionSequence: number;
}

type WebAutoBatchResult =
  | { kind: "success"; rewriteUnitIds: string[]; completed: CompletedRewriteUnitPayload[] }
  | { kind: "failed"; rewriteUnitIds: string[]; error: string }
  | { kind: "cancelled"; rewriteUnitIds: string[] };

export interface WebAutoJob {
  sessionId: string;
  queue: string[];
  targetUnitIds: Set<string> | null;
  completedUnits: number;
  totalUnits: number;
  paused: boolean;
  cancelled: boolean;
  nextBatchToken: number;
  controllers: Map<number, AbortController>;
}

interface AutoJobRuntimeDeps {
  sessions: Map<string, DocumentSession>;
  autoJobs: Map<string, WebAutoJob>;
  getSettings: () => { maxConcurrency: number; unitsPerBatch: number };
  processRewriteBatch: (params: {
    session: DocumentSession;
    rewriteUnitIds: string[];
    autoApprove: boolean;
    signal?: AbortSignal;
  }) => Promise<CompletedRewriteUnitPayload[]>;
  markUnitsRunning: (session: DocumentSession, rewriteUnitIds: string[]) => void;
  markBatchFailure: (session: DocumentSession, rewriteUnitIds: string[], error: string) => void;
  clearRunningUnits: (session: DocumentSession) => void;
  updateSessionTimestamp: (session: DocumentSession) => void;
  computeSessionState: (session: DocumentSession) => RunningState;
}

function sleep(ms: number) {
  return new Promise<void>((resolve) => {
    window.setTimeout(() => resolve(), ms);
  });
}

export function createAutoJobRuntime(deps: AutoJobRuntimeDeps) {
  async function emitRewriteProgress(
    session: DocumentSession,
    job: WebAutoJob,
    runningState: RunningState
  ) {
    const payload: RewriteProgress = {
      sessionId: session.id,
      completedUnits: job.completedUnits,
      inFlight: job.controllers.size,
      runningUnitIds: Array.from(
        new Set(
          session.rewriteUnits
            .filter((unit) => unit.status === "running")
            .map((unit) => unit.id)
        )
      ).sort(),
      totalUnits: job.totalUnits,
      mode: "auto",
      runningState,
      maxConcurrency: deps.getSettings().maxConcurrency
    };
    await emitRuntimeEvent(TAURI_EVENTS.REWRITE_PROGRESS, payload);
  }

  async function emitRewriteUnitCompleted(
    sessionId: string,
    completed: CompletedRewriteUnitPayload[]
  ) {
    for (const item of completed) {
      const payload: RewriteUnitCompletedPayload = {
        sessionId,
        rewriteUnitId: item.rewriteUnitId,
        suggestionId: item.suggestionId,
        suggestionSequence: item.suggestionSequence
      };
      await emitRuntimeEvent(TAURI_EVENTS.REWRITE_UNIT_COMPLETED, payload);
    }
  }

  async function emitRewriteFinished(sessionId: string) {
    const payload: SessionEventPayload = { sessionId };
    await emitRuntimeEvent(TAURI_EVENTS.REWRITE_FINISHED, payload);
  }

  async function emitRewriteFailed(sessionId: string, error: string) {
    const payload: RewriteFailedPayload = { sessionId, error };
    await emitRuntimeEvent(TAURI_EVENTS.REWRITE_FAILED, payload);
  }

  async function runAutoJobLoop(sessionId: string) {
    const job = deps.autoJobs.get(sessionId);
    if (!job) {
      return;
    }

    while (true) {
      const session = deps.sessions.get(sessionId);
      if (!session) {
        deps.autoJobs.delete(sessionId);
        return;
      }

      if (job.cancelled) {
        deps.clearRunningUnits(session);
        session.status = "cancelled";
        deps.updateSessionTimestamp(session);
        await emitRewriteProgress(session, job, "cancelled");
        await emitRewriteFinished(sessionId);
        deps.autoJobs.delete(sessionId);
        return;
      }

      if (job.paused) {
        session.status = "paused";
        deps.updateSessionTimestamp(session);
        await emitRewriteProgress(session, job, "paused");
        await sleep(160);
        continue;
      }

      session.status = "running";
      deps.updateSessionTimestamp(session);

      while (
        job.controllers.size < deps.getSettings().maxConcurrency &&
        job.queue.length > 0 &&
        !job.paused &&
        !job.cancelled
      ) {
        const batchSize = Math.max(1, deps.getSettings().unitsPerBatch);
        const rewriteUnitIds = job.queue.splice(0, batchSize);
        deps.markUnitsRunning(session, rewriteUnitIds);
        const token = job.nextBatchToken;
        job.nextBatchToken += 1;
        const controller = new AbortController();
        job.controllers.set(token, controller);
        void emitRewriteProgress(session, job, "running");

        const promise = deps
          .processRewriteBatch({
            session,
            rewriteUnitIds,
            autoApprove: true,
            signal: controller.signal
          })
          .then(
            (completed): WebAutoBatchResult => ({
              kind: "success",
              rewriteUnitIds,
              completed
            })
          )
          .catch((error): WebAutoBatchResult => {
            if (controller.signal.aborted || job.cancelled) {
              return { kind: "cancelled", rewriteUnitIds };
            }
            return {
              kind: "failed",
              rewriteUnitIds,
              error: error instanceof Error ? error.message : String(error)
            };
          })
          .finally(() => {
            job.controllers.delete(token);
          });

        void (async () => {
          const result = await promise;
          const activeSession = deps.sessions.get(sessionId);
          if (!activeSession) {
            return;
          }
          if (result.kind === "cancelled") {
            deps.clearRunningUnits(activeSession);
            deps.updateSessionTimestamp(activeSession);
            return;
          }
          if (result.kind === "failed") {
            deps.markBatchFailure(activeSession, result.rewriteUnitIds, result.error);
            await emitRewriteFailed(sessionId, result.error);
            deps.autoJobs.delete(sessionId);
            return;
          }
          job.completedUnits += result.completed.length;
          await emitRewriteUnitCompleted(sessionId, result.completed);
          await emitRewriteProgress(activeSession, job, "running");
        })();
      }

      if (job.controllers.size === 0 && job.queue.length === 0) {
        deps.clearRunningUnits(session);
        session.status = deps.computeSessionState(session);
        deps.updateSessionTimestamp(session);
        await emitRewriteProgress(session, job, session.status);
        await emitRewriteFinished(sessionId);
        deps.autoJobs.delete(sessionId);
        return;
      }

      await sleep(120);
    }
  }

  return {
    emitRewriteProgress,
    emitRewriteUnitCompleted,
    runAutoJobLoop
  };
}
