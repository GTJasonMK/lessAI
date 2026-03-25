import { memo } from "react";
import { Check, LoaderCircle, RotateCcw, Trash2, X } from "lucide-react";
import type { ChunkTask, EditSuggestion } from "../../../lib/types";
import {
  chunkStatusTone,
  formatChunkStatus,
  formatSuggestionDecision,
  suggestionTone
} from "../../../lib/helpers";
import { StatusBadge } from "../../../components/StatusBadge";

interface ReviewActionBarProps {
  editorMode: boolean;
  settingsReady: boolean;
  rewriteRunning: boolean;
  rewritePaused: boolean;
  anyBusy: boolean;
  busyAction: string | null;
  activeChunk: ChunkTask | null;
  activeChunkSuggestions: EditSuggestion[];
  activeSuggestion: EditSuggestion | null;
  onRetry: () => void;
  onApplySuggestion: (suggestionId: string) => void;
  onDismissSuggestion: (suggestionId: string) => void;
  onDeleteSuggestion: (suggestionId: string) => void;
}

export const ReviewActionBar = memo(function ReviewActionBar({
  editorMode,
  settingsReady,
  rewriteRunning,
  rewritePaused,
  anyBusy,
  busyAction,
  activeChunk,
  activeChunkSuggestions,
  activeSuggestion,
  onRetry,
  onApplySuggestion,
  onDismissSuggestion,
  onDeleteSuggestion
}: ReviewActionBarProps) {
  return (
    <div className={`workbench-action-reel ${editorMode ? "is-editor" : ""}`}>
      <div className="workbench-action-track">
        <div
          className="workbench-review-actionbar workbench-action-row is-normal"
          aria-hidden={editorMode}
        >
          <div className="workbench-review-actionbar-status">
            {activeSuggestion ? (
              <StatusBadge tone={suggestionTone(activeSuggestion.decision)}>
                #{activeSuggestion.sequence} {formatSuggestionDecision(activeSuggestion.decision)}
              </StatusBadge>
            ) : activeChunk ? (
              <StatusBadge tone={chunkStatusTone(activeChunk, activeChunkSuggestions)}>
                {formatChunkStatus(activeChunk, activeChunkSuggestions)}
              </StatusBadge>
            ) : (
              <StatusBadge tone={settingsReady ? "info" : "warning"}>
                {settingsReady ? "等待生成" : "未配置"}
              </StatusBadge>
            )}
          </div>

          <div className="workbench-review-actionbar-buttons">
            {activeChunk?.status === "failed" ? (
              <button
                type="button"
                className="icon-button icon-button-sm"
                onClick={onRetry}
                aria-label="重试生成当前位置"
                title="重试生成当前位置"
                disabled={
                  editorMode ||
                  !settingsReady ||
                  rewriteRunning ||
                  rewritePaused ||
                  busyAction === "retry-chunk" ||
                  (anyBusy && busyAction !== "retry-chunk")
                }
              >
                {busyAction === "retry-chunk" ? (
                  <LoaderCircle className="spin" />
                ) : (
                  <RotateCcw />
                )}
              </button>
            ) : null}

            {activeSuggestion ? (
              <>
                <button
                  type="button"
                  className="icon-button icon-button-sm"
                  onClick={() => onApplySuggestion(activeSuggestion.id)}
                  aria-label="应用该修改对"
                  title="应用"
                  disabled={
                    editorMode ||
                    rewriteRunning ||
                    rewritePaused ||
                    activeSuggestion.decision === "applied" ||
                    busyAction === `apply-suggestion:${activeSuggestion.id}` ||
                    (anyBusy && busyAction !== `apply-suggestion:${activeSuggestion.id}`)
                  }
                >
                  {busyAction === `apply-suggestion:${activeSuggestion.id}` ? (
                    <LoaderCircle className="spin" />
                  ) : (
                    <Check />
                  )}
                </button>
                <button
                  type="button"
                  className="icon-button icon-button-sm"
                  onClick={() => onDismissSuggestion(activeSuggestion.id)}
                  aria-label={
                    activeSuggestion.decision === "applied" ? "取消应用该修改对" : "忽略该修改对"
                  }
                  title={activeSuggestion.decision === "applied" ? "取消应用" : "忽略"}
                  disabled={
                    editorMode ||
                    rewriteRunning ||
                    rewritePaused ||
                    activeSuggestion.decision === "dismissed" ||
                    busyAction === `dismiss-suggestion:${activeSuggestion.id}` ||
                    (anyBusy && busyAction !== `dismiss-suggestion:${activeSuggestion.id}`)
                  }
                >
                  {busyAction === `dismiss-suggestion:${activeSuggestion.id}` ? (
                    <LoaderCircle className="spin" />
                  ) : activeSuggestion.decision === "applied" ? (
                    <RotateCcw />
                  ) : (
                    <X />
                  )}
                </button>
                <button
                  type="button"
                  className="icon-button icon-button-sm"
                  onClick={() => onDeleteSuggestion(activeSuggestion.id)}
                  aria-label="删除该修改对"
                  title="删除"
                  disabled={
                    editorMode ||
                    rewriteRunning ||
                    rewritePaused ||
                    busyAction === `delete-suggestion:${activeSuggestion.id}` ||
                    (anyBusy && busyAction !== `delete-suggestion:${activeSuggestion.id}`)
                  }
                >
                  {busyAction === `delete-suggestion:${activeSuggestion.id}` ? (
                    <LoaderCircle className="spin" />
                  ) : (
                    <Trash2 />
                  )}
                </button>
              </>
            ) : null}
          </div>
        </div>

        <div
          className="workbench-review-actionbar workbench-action-row is-editor"
          aria-hidden={!editorMode}
        >
          <div className="workbench-review-actionbar-status">
            <StatusBadge tone="info">编辑模式</StatusBadge>
          </div>
          <div className="workbench-review-actionbar-buttons">
            <StatusBadge tone="info">审阅只读</StatusBadge>
          </div>
        </div>
      </div>
    </div>
  );
});

