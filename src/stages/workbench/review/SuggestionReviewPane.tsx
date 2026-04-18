import { memo, useMemo, useState } from "react";
import { AlertCircle, RotateCcw } from "lucide-react";
import { logScrollRestore } from "../../../app/hooks/documentScrollRestoreDebug";
import type { DocumentSession, RewriteSuggestion, RewriteUnit } from "../../../lib/types";
import type { SessionStats } from "../../../lib/helpers";
import { ReviewSuggestionRow } from "./ReviewSuggestionRow";
import { buildSuggestionRowActionState } from "./reviewSuggestionRowModel";

interface SuggestionReviewPaneProps {
  settingsReady: boolean;
  currentSession: DocumentSession;
  currentStats: SessionStats;
  activeRewriteUnit: RewriteUnit | null;
  activeSuggestionId: string | null;
  orderedSuggestions: RewriteSuggestion[];
  anyBusy: boolean;
  busyAction: string | null;
  rewriteRunning: boolean;
  rewritePaused: boolean;
  onSelectRewriteUnit: (rewriteUnitId: string, options?: { multiSelect?: boolean }) => void;
  onSelectSuggestion: (suggestionId: string, options?: { forceScroll?: boolean }) => void;
  onApplySuggestion: (suggestionId: string) => void;
  onDismissSuggestion: (suggestionId: string) => void;
  onDeleteSuggestion: (suggestionId: string) => void;
  onRetry: () => void;
}

export const SuggestionReviewPane = memo(function SuggestionReviewPane({
  settingsReady,
  currentSession,
  currentStats,
  activeRewriteUnit,
  activeSuggestionId,
  orderedSuggestions,
  anyBusy,
  busyAction,
  rewriteRunning,
  rewritePaused,
  onSelectRewriteUnit,
  onSelectSuggestion,
  onApplySuggestion,
  onDismissSuggestion,
  onDeleteSuggestion,
  onRetry
}: SuggestionReviewPaneProps) {
  const [openMenuSuggestionId, setOpenMenuSuggestionId] = useState<string | null>(null);
  const failedRewriteUnitIds = useMemo(
    () =>
      new Set(
        currentSession.rewriteUnits
          .filter((rewriteUnit) => rewriteUnit.status === "failed")
          .map((rewriteUnit) => rewriteUnit.id)
      ),
    [currentSession.rewriteUnits]
  );

  return (
    <>
      <div className="review-summary-strip">
        <span className="context-chip">建议：{currentStats.suggestionsTotal}</span>
        <span className="context-chip">待处理：{currentStats.unitsProposed}</span>
        <span className="context-chip">
          已应用：{currentStats.unitsApplied}/{currentStats.total}
        </span>
      </div>

      {activeRewriteUnit?.status === "failed" && !activeSuggestionId ? (
        <div className="error-card">
          <AlertCircle />
          <div>
            <strong>该片段生成失败</strong>
            <span>{activeRewriteUnit.errorMessage ?? "请点击重试重新生成。"}</span>
          </div>
          <button
            type="button"
            className="icon-button icon-button-sm"
            onClick={onRetry}
            disabled={
              !settingsReady ||
              rewriteRunning ||
              rewritePaused ||
              busyAction === "retry-rewrite-unit" ||
              (anyBusy && busyAction !== "retry-rewrite-unit")
            }
          >
            {busyAction === "retry-rewrite-unit" ? (
              <RotateCcw className="spin" />
            ) : (
              <RotateCcw />
            )}
          </button>
        </div>
      ) : null}

      {orderedSuggestions.length === 0 ? (
        <div className="empty-inline">
          <span>还没有建议。点击左侧「文档」右上角的“开始优化”生成一段。</span>
        </div>
      ) : (
        <div className="suggestion-list scroll-region">
          {orderedSuggestions.map((suggestion) => (
            <ReviewSuggestionRow
              key={suggestion.id}
              suggestion={suggestion}
              active={suggestion.id === activeSuggestionId}
              menuOpen={openMenuSuggestionId === suggestion.id}
              actionState={buildSuggestionRowActionState({
                suggestionId: suggestion.id,
                decision: suggestion.decision,
                busyAction,
                anyBusy,
                editorMode: false,
                rewriteRunning,
                rewritePaused,
                settingsReady,
                rewriteUnitFailed: failedRewriteUnitIds.has(suggestion.rewriteUnitId)
              })}
              onSelect={() => {
                logScrollRestore("review-row-select", {
                  sessionId: currentSession.id,
                  clickedSuggestionId: suggestion.id,
                  clickedRewriteUnitId: suggestion.rewriteUnitId,
                  currentActiveSuggestionId: activeSuggestionId,
                  currentActiveRewriteUnitId: activeRewriteUnit?.id ?? null
                });
                setOpenMenuSuggestionId(null);
                onSelectRewriteUnit(suggestion.rewriteUnitId);
                onSelectSuggestion(suggestion.id, { forceScroll: true });
              }}
              onApply={() => {
                setOpenMenuSuggestionId(null);
                onApplySuggestion(suggestion.id);
              }}
              onDelete={() => {
                setOpenMenuSuggestionId(null);
                onDeleteSuggestion(suggestion.id);
              }}
              onDismiss={() => {
                setOpenMenuSuggestionId(null);
                onDismissSuggestion(suggestion.id);
              }}
              onRetry={() => {
                setOpenMenuSuggestionId(null);
                onRetry();
              }}
              onToggleMenu={() =>
                setOpenMenuSuggestionId((current) =>
                  current === suggestion.id ? null : suggestion.id
                )
              }
            />
          ))}
        </div>
      )}
    </>
  );
});
