import { useEffect, useRef } from "react";
import { TAURI_EVENTS } from "../lib/constants";
import type {
  RewriteFailedPayload,
  RewriteUnitCompletedPayload,
  SessionEventPayload
} from "../lib/constants";
import type { RewriteProgress } from "../lib/types";
import { listenRuntimeEvent } from "../lib/runtimeEvents";

interface TauriEventHandlers {
  onProgress: (payload: RewriteProgress) => void;
  onRewriteUnitCompleted: (payload: RewriteUnitCompletedPayload) => void;
  onFinished: (payload: SessionEventPayload) => void;
  onFailed: (payload: RewriteFailedPayload) => void;
}

export function useTauriEvents(handlers: TauriEventHandlers) {
  const handlersRef = useRef(handlers);
  handlersRef.current = handlers;

  useEffect(() => {
    let mounted = true;
    let cleanup: (() => void) | null = null;

    void (async () => {
      const unlisteners = await Promise.all([
        listenRuntimeEvent<RewriteProgress>(TAURI_EVENTS.REWRITE_PROGRESS, ({ payload }) => {
          handlersRef.current.onProgress(payload);
        }),
        listenRuntimeEvent<RewriteUnitCompletedPayload>(TAURI_EVENTS.REWRITE_UNIT_COMPLETED, ({ payload }) => {
          handlersRef.current.onRewriteUnitCompleted(payload);
        }),
        listenRuntimeEvent<SessionEventPayload>(TAURI_EVENTS.REWRITE_FINISHED, ({ payload }) => {
          handlersRef.current.onFinished(payload);
        }),
        listenRuntimeEvent<RewriteFailedPayload>(TAURI_EVENTS.REWRITE_FAILED, ({ payload }) => {
          handlersRef.current.onFailed(payload);
        })
      ]);

      if (!mounted) {
        for (const unlisten of unlisteners) {
          void unlisten();
        }
        return;
      }

      cleanup = () => {
        for (const unlisten of unlisteners) {
          void unlisten();
        }
      };
    })();

    return () => {
      mounted = false;
      cleanup?.();
    };
  }, []);
}
