import { memo } from "react";
import type { DocumentSession, RewriteSuggestion, RewriteUnit } from "../../lib/types";
import type { SessionStats } from "../../lib/helpers";
import type { EditorSlotOverrides } from "../../lib/editorSlots";
import { Panel } from "../../components/Panel";
import { EditorReviewPane } from "./review/EditorReviewPane";
import { ReviewActionBar } from "./review/ReviewActionBar";
import { ReviewEmptyState } from "./review/ReviewEmptyState";
import { SuggestionReviewPane } from "./review/SuggestionReviewPane";

interface ReviewPanelProps {
  settingsReady: boolean;
  currentSession: DocumentSession | null;
  currentStats: SessionStats | null;
  activeRewriteUnit: RewriteUnit | null;
  activeRewriteUnitSuggestions: RewriteSuggestion[];
  activeSuggestionId: string | null;
  activeSuggestion: RewriteSuggestion | null;
  showMarkers: boolean;
  busyAction: string | null;
  editorMode: boolean;
  editorText: string;
  editorSlotOverrides: EditorSlotOverrides;
  editorDirty: boolean;
  orderedSuggestions: RewriteSuggestion[];
  onOpenSettings: () => void;
  onSelectRewriteUnit: (rewriteUnitId: string, options?: { multiSelect?: boolean }) => void;
  onSelectSuggestion: (suggestionId: string, options?: { forceScroll?: boolean }) => void;
  onApplySuggestion: (suggestionId: string) => void;
  onDismissSuggestion: (suggestionId: string) => void;
  onDeleteSuggestion: (suggestionId: string) => void;
  onRetry: () => void;
}

export const ReviewPanel = memo(function ReviewPanel({
  settingsReady,
  currentSession,
  currentStats,
  activeRewriteUnit,
  activeRewriteUnitSuggestions,
  activeSuggestionId,
  activeSuggestion,
  showMarkers,
  busyAction,
  editorMode,
  editorText,
  editorSlotOverrides,
  editorDirty,
  orderedSuggestions,
  onOpenSettings,
  onSelectRewriteUnit,
  onSelectSuggestion,
  onApplySuggestion,
  onDismissSuggestion,
  onDeleteSuggestion,
  onRetry
}: ReviewPanelProps) {
  const anyBusy = Boolean(busyAction);
  const rewriteRunning = currentSession?.status === "running";
  const rewritePaused = currentSession?.status === "paused";

  return (
    <Panel
      title="建议"
      subtitle="建议列表"
      className="workbench-review-panel"
      bodyClassName="workbench-review-body"
      action={
        <ReviewActionBar
          editorMode={editorMode}
          settingsReady={settingsReady}
          currentSession={currentSession}
          activeRewriteUnit={activeRewriteUnit}
          activeRewriteUnitSuggestions={activeRewriteUnitSuggestions}
          activeSuggestion={activeSuggestion}
        />
      }
    >
      {currentSession && currentStats ? (
        <div className={`workbench-mode-switch workbench-review-mode-switch ${editorMode ? "is-editor" : ""}`}>
          <div
            className="workbench-mode-pane is-normal"
            aria-hidden={editorMode}
            inert={editorMode}
          >
            <div className="workbench-mode-content workbench-review-mode-content">
              <SuggestionReviewPane
                settingsReady={settingsReady}
                currentSession={currentSession}
                currentStats={currentStats}
                activeRewriteUnit={activeRewriteUnit}
                activeSuggestionId={activeSuggestionId}
                orderedSuggestions={orderedSuggestions}
                anyBusy={anyBusy}
                busyAction={busyAction}
                rewriteRunning={rewriteRunning ?? false}
                rewritePaused={rewritePaused ?? false}
                onSelectRewriteUnit={onSelectRewriteUnit}
                onSelectSuggestion={onSelectSuggestion}
                onApplySuggestion={onApplySuggestion}
                onDismissSuggestion={onDismissSuggestion}
                onDeleteSuggestion={onDeleteSuggestion}
                onRetry={onRetry}
              />
            </div>
          </div>

          <div
            className="workbench-mode-pane is-editor"
            aria-hidden={!editorMode}
            inert={!editorMode}
          >
            <div className="workbench-mode-content workbench-review-mode-content">
              <EditorReviewPane
                currentSession={currentSession}
                editorText={editorText}
                editorSlotOverrides={editorSlotOverrides}
                editorDirty={editorDirty}
                showMarkers={showMarkers}
              />
            </div>
          </div>
        </div>
      ) : (
        <ReviewEmptyState
          settingsReady={settingsReady}
          onOpenSettings={onOpenSettings}
        />
      )}
    </Panel>
  );
});
