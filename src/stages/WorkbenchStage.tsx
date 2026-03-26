import { memo, useEffect, useMemo, useState } from "react";
import type {
  AppSettings,
  ChunkTask,
  DocumentSession,
  EditSuggestion,
  RewriteMode,
  RewriteProgress
} from "../lib/types";
import type { SessionStats } from "../lib/helpers";
import type { ReviewView } from "../lib/constants";
import { groupSuggestionsByChunk, isSettingsReady } from "../lib/helpers";
import { DocumentPanel } from "./workbench/DocumentPanel";
import { ReviewPanel } from "./workbench/ReviewPanel";

interface WorkbenchStageProps {
  settings: AppSettings;
  currentSession: DocumentSession | null;
  liveProgress: RewriteProgress | null;
  currentStats: SessionStats | null;
  activeChunk: ChunkTask | null;
  activeChunkIndex: number;
  activeSuggestionId: string | null;
  reviewView: ReviewView;
  busyAction: string | null;
  editorMode: boolean;
  editorText: string;
  editorDirty: boolean;
  onOpenDocument: () => void;
  onSelectChunk: (index: number) => void;
  onSelectSuggestion: (suggestionId: string) => void;
  onSetReviewView: (view: ReviewView) => void;
  onStartRewrite: (mode: RewriteMode) => void;
  onPause: () => void;
  onResume: () => void;
  onCancel: () => void;
  onFinalizeDocument: () => void;
  onResetSession: () => void;
  onApplySuggestion: (suggestionId: string) => void;
  onDismissSuggestion: (suggestionId: string) => void;
  onDeleteSuggestion: (suggestionId: string) => void;
  onRetry: () => void;
  onOpenSettings: () => void;
  onEnterEditor: () => void;
  onChangeEditorText: (value: string) => void;
  onSaveEditor: () => void;
  onSaveEditorAndExit: () => void;
  onDiscardEditorChanges: () => void;
  onExitEditor: () => void;
}

export const WorkbenchStage = memo(function WorkbenchStage({
  settings,
  currentSession,
  liveProgress,
  currentStats,
  activeChunk,
  activeChunkIndex,
  activeSuggestionId,
  reviewView,
  busyAction,
  editorMode,
  editorText,
  editorDirty,
  onOpenDocument,
  onSelectChunk,
  onSelectSuggestion,
  onSetReviewView,
  onStartRewrite,
  onPause,
  onResume,
  onCancel,
  onFinalizeDocument,
  onResetSession,
  onApplySuggestion,
  onDismissSuggestion,
  onDeleteSuggestion,
  onRetry,
  onOpenSettings,
  onEnterEditor,
  onChangeEditorText,
  onSaveEditor,
  onSaveEditorAndExit,
  onDiscardEditorChanges,
  onExitEditor
}: WorkbenchStageProps) {
  const settingsReady = isSettingsReady(settings);

  const [showMarkers, setShowMarkers] = useState<boolean>(() => {
    try {
      const raw =
        typeof localStorage === "undefined" ? null : localStorage.getItem("lessai.showMarkers");
      // 默认开启：分块边界/保护区/运行态/差异高亮是工作台的核心可视化信息。
      // 用户仍可手动关闭以获得更“通读”的视图。
      if (!raw) return true;
      return raw === "1" || raw.toLowerCase() === "true";
    } catch {
      return true;
    }
  });

  useEffect(() => {
    try {
      if (typeof localStorage === "undefined") return;
      localStorage.setItem("lessai.showMarkers", showMarkers ? "1" : "0");
    } catch {
      // ignore
    }
  }, [showMarkers]);

  const suggestionsByChunk = useMemo(
    () => groupSuggestionsByChunk(currentSession?.suggestions ?? []),
    [currentSession?.suggestions]
  );

  const runningIndexSet = useMemo(() => {
    if (!currentSession) return new Set<number>();
    if (!liveProgress) return new Set<number>();
    if (liveProgress.sessionId !== currentSession.id) return new Set<number>();
    return new Set(liveProgress.runningIndices);
  }, [currentSession, liveProgress]);

  const optimisticManualRunningIndex = useMemo(() => {
    if (!currentSession) return null;
    if (busyAction === "retry-chunk") {
      return currentSession.chunks[activeChunkIndex]?.index ?? null;
    }
    if (busyAction !== "start-manual") {
      return null;
    }
    const target = currentSession.chunks.find(
      (chunk) => chunk.status === "idle" || chunk.status === "failed"
    );
    return target?.index ?? null;
  }, [activeChunkIndex, busyAction, currentSession]);

  const activeChunkSuggestions = useMemo(() => {
    if (!currentSession || !activeChunk) return [];
    return suggestionsByChunk.get(activeChunk.index) ?? [];
  }, [activeChunk, currentSession, suggestionsByChunk]);

  const orderedSuggestions = useMemo(() => {
    if (!currentSession) return [];
    return [...currentSession.suggestions].sort((a, b) => a.sequence - b.sequence);
  }, [currentSession]);

  const activeSuggestion = useMemo<EditSuggestion | null>(() => {
    if (!currentSession || !activeSuggestionId) return null;
    return currentSession.suggestions.find((item) => item.id === activeSuggestionId) ?? null;
  }, [currentSession, activeSuggestionId]);

  return (
    <div className="workbench-root">
      <div className="workbench-layout">
        <div className="workbench-column is-center">
          <DocumentPanel
            settings={settings}
            settingsReady={settingsReady}
            currentSession={currentSession}
            currentStats={currentStats}
            showMarkers={showMarkers}
            suggestionsByChunk={suggestionsByChunk}
            runningIndexSet={runningIndexSet}
            optimisticManualRunningIndex={optimisticManualRunningIndex}
            activeChunkIndex={activeChunkIndex}
            busyAction={busyAction}
            editorMode={editorMode}
            editorText={editorText}
            editorDirty={editorDirty}
            onOpenDocument={onOpenDocument}
            onOpenSettings={onOpenSettings}
            onSelectChunk={onSelectChunk}
            onSelectSuggestion={onSelectSuggestion}
            onStartRewrite={onStartRewrite}
            onPause={onPause}
            onResume={onResume}
            onCancel={onCancel}
            onFinalizeDocument={onFinalizeDocument}
            onResetSession={onResetSession}
            onEnterEditor={onEnterEditor}
            onChangeEditorText={onChangeEditorText}
            onSaveEditor={onSaveEditor}
            onSaveEditorAndExit={onSaveEditorAndExit}
            onDiscardEditorChanges={onDiscardEditorChanges}
            onExitEditor={onExitEditor}
            onToggleMarkers={() => setShowMarkers((value) => !value)}
          />
        </div>

        <div className="workbench-column is-right">
          <ReviewPanel
            settingsReady={settingsReady}
            currentSession={currentSession}
            currentStats={currentStats}
            activeChunk={activeChunk}
            activeChunkSuggestions={activeChunkSuggestions}
            activeSuggestionId={activeSuggestionId}
            activeSuggestion={activeSuggestion}
            showMarkers={showMarkers}
            busyAction={busyAction}
            editorMode={editorMode}
            editorText={editorText}
            editorDirty={editorDirty}
            reviewView={reviewView}
            orderedSuggestions={orderedSuggestions}
            onOpenDocument={onOpenDocument}
            onOpenSettings={onOpenSettings}
            onSelectChunk={onSelectChunk}
            onSelectSuggestion={onSelectSuggestion}
            onSetReviewView={onSetReviewView}
            onApplySuggestion={onApplySuggestion}
            onDismissSuggestion={onDismissSuggestion}
            onDeleteSuggestion={onDeleteSuggestion}
            onRetry={onRetry}
          />
        </div>
      </div>
    </div>
  );
});
