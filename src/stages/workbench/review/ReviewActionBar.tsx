import { memo } from "react";
import type { DocumentSession, RewriteSuggestion, RewriteUnit } from "../../../lib/types";
import {
  formatRewriteUnitStatus,
  formatSuggestionDecision,
  rewriteUnitStatusTone,
  suggestionTone
} from "../../../lib/helpers";
import { StatusBadge } from "../../../components/StatusBadge";

interface ReviewActionBarProps {
  editorMode: boolean;
  settingsReady: boolean;
  currentSession: DocumentSession | null;
  activeRewriteUnit: RewriteUnit | null;
  activeRewriteUnitSuggestions: RewriteSuggestion[];
  activeSuggestion: RewriteSuggestion | null;
}

export const ReviewActionBar = memo(function ReviewActionBar({
  editorMode,
  settingsReady,
  currentSession,
  activeRewriteUnit,
  activeRewriteUnitSuggestions,
  activeSuggestion
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
            ) : currentSession && activeRewriteUnit ? (
              <StatusBadge
                tone={rewriteUnitStatusTone(
                  currentSession,
                  activeRewriteUnit,
                  activeRewriteUnitSuggestions
                )}
              >
                {formatRewriteUnitStatus(
                  currentSession,
                  activeRewriteUnit,
                  activeRewriteUnitSuggestions
                )}
              </StatusBadge>
            ) : (
              <StatusBadge tone={settingsReady ? "info" : "warning"}>
                {settingsReady ? "等待生成" : "未配置"}
              </StatusBadge>
            )}
          </div>
        </div>

        <div
          className="workbench-review-actionbar workbench-action-row is-editor"
          aria-hidden={!editorMode}
        >
          <div className="workbench-review-actionbar-status">
            <StatusBadge tone="info">编辑模式</StatusBadge>
          </div>
        </div>
      </div>
    </div>
  );
});
