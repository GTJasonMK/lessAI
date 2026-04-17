import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { TAURI_EVENTS } from "../lib/constants";
import type {
  RewriteFailedPayload,
  RewriteUnitCompletedPayload,
  SessionEventPayload
} from "../lib/constants";
import type { RewriteProgress } from "../lib/types";

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
        listen<RewriteProgress>(TAURI_EVENTS.REWRITE_PROGRESS, ({ payload }) => {
          handlersRef.current.onProgress(payload);
        }),
        listen<RewriteUnitCompletedPayload>(TAURI_EVENTS.REWRITE_UNIT_COMPLETED, ({ payload }) => {
          handlersRef.current.onRewriteUnitCompleted(payload);
        }),
        listen<SessionEventPayload>(TAURI_EVENTS.REWRITE_FINISHED, ({ payload }) => {
          handlersRef.current.onFinished(payload);
        }),
        listen<RewriteFailedPayload>(TAURI_EVENTS.REWRITE_FAILED, ({ payload }) => {
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
