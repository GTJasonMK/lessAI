import { memo } from "react";
import type { DocumentSession, RewriteSuggestion, RewriteUnit } from "../../lib/types";
import type { SessionStats } from "../../lib/helpers";
import type { ReviewView } from "../../lib/constants";
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
  editorDirty: boolean;
  reviewView: ReviewView;
  orderedSuggestions: RewriteSuggestion[];
  onOpenDocument: () => void;
  onOpenSettings: () => void;
  onSelectRewriteUnit: (rewriteUnitId: string, options?: { multiSelect?: boolean }) => void;
  onSelectSuggestion: (suggestionId: string) => void;
  onSetReviewView: (view: ReviewView) => void;
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
  editorDirty,
  reviewView,
  orderedSuggestions,
  onOpenDocument,
  onOpenSettings,
  onSelectRewriteUnit,
  onSelectSuggestion,
  onSetReviewView,
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
      title="审阅"
      subtitle="修改对时间线"
      className="workbench-review-panel"
      bodyClassName="workbench-review-body"
      action={
        <ReviewActionBar
          editorMode={editorMode}
          settingsReady={settingsReady}
          rewriteRunning={rewriteRunning ?? false}
          rewritePaused={rewritePaused ?? false}
          anyBusy={anyBusy}
          busyAction={busyAction}
          currentSession={currentSession}
          activeRewriteUnit={activeRewriteUnit}
          activeRewriteUnitSuggestions={activeRewriteUnitSuggestions}
          activeSuggestion={activeSuggestion}
          onRetry={onRetry}
          onApplySuggestion={onApplySuggestion}
          onDismissSuggestion={onDismissSuggestion}
          onDeleteSuggestion={onDeleteSuggestion}
        />
      }
    >
      {currentSession && currentStats ? (
        editorMode ? (
          <EditorReviewPane
            currentSession={currentSession}
            editorText={editorText}
            editorDirty={editorDirty}
            showMarkers={showMarkers}
          />
        ) : (
          <SuggestionReviewPane
            currentSession={currentSession}
            currentStats={currentStats}
            activeRewriteUnit={activeRewriteUnit}
            activeSuggestionId={activeSuggestionId}
            activeSuggestion={activeSuggestion}
            showMarkers={showMarkers}
            reviewView={reviewView}
            orderedSuggestions={orderedSuggestions}
            onSetReviewView={onSetReviewView}
            onSelectRewriteUnit={onSelectRewriteUnit}
            onSelectSuggestion={onSelectSuggestion}
          />
        )
      ) : (
        <ReviewEmptyState
          busyAction={busyAction}
          anyBusy={anyBusy}
          settingsReady={settingsReady}
          onOpenDocument={onOpenDocument}
          onOpenSettings={onOpenSettings}
        />
      )}
    </Panel>
  );
});
