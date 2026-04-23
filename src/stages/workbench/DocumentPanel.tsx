import { memo, useCallback, useMemo, useState } from "react";
import type { MutableRefObject } from "react";
import type {
  AppSettings,
  DocumentSession,
  RewriteSuggestion
} from "../../lib/types";
import type { EditorSlotOverrides } from "../../lib/editorSlots";
import type { SessionStats } from "../../lib/helpers";
import {
  documentBackendKind,
  documentEditorMode,
  editorEntryBlockedReason,
  sessionIsClean,
  sessionSupportsSourceWriteback
} from "../../lib/documentCapabilities";
import {
  canRewriteSession,
  countCharacters,
  rewriteBlockedReason
} from "../../lib/helpers";
import {
  countSelectedRewriteUnits,
  findAutoPendingTargetRewriteUnits,
  findNextManualTargetRewriteUnit,
  hasSelectedRewriteUnits
} from "../../lib/rewriteUnitSelection";
import { guessClientDocumentFormat } from "../../lib/protectedText";
import { Panel } from "../../components/Panel";
import { useCopyDocument, type DocumentView } from "./hooks/useCopyDocument";
import { DocumentActionBar } from "./document/DocumentActionBar";
import { DocumentEditor, type DocumentEditorHandle } from "./document/DocumentEditor";
import { DocumentEmptyState } from "./document/DocumentEmptyState";
import { DocumentFlow } from "./document/DocumentFlow";

interface DocumentPanelProps {
  settings: AppSettings;
  settingsReady: boolean;
  currentSession: DocumentSession | null;
  currentStats: SessionStats | null;
  showMarkers: boolean;
  suggestionsByRewriteUnit: Map<string, RewriteSuggestion[]>;
  runningRewriteUnitIdSet: Set<string>;
  optimisticManualRunningRewriteUnitId: string | null;
  activeRewriteUnitId: string | null;
  activeSuggestionId: string | null;
  activeReviewNavigationRequestId: number;
  selectedRewriteUnitIds: string[];
  busyAction: string | null;
  editorMode: boolean;
  editorText: string;
  editorSlotOverrides: EditorSlotOverrides;
  editorDirty: boolean;
  editorHasSelection: boolean;
  editorRef: MutableRefObject<DocumentEditorHandle | null>;
  documentScrollRef: MutableRefObject<HTMLDivElement | null>;
  onOpenDocument: () => void;
  onOpenSettings: () => void;
  onSelectRewriteUnit: (rewriteUnitId: string, options?: { multiSelect?: boolean }) => void;
  onSelectSuggestion: (suggestionId: string, options?: { forceScroll?: boolean }) => void;
  onStartRewrite: (mode: AppSettings["rewriteMode"]) => void;
  onPause: () => void;
  onResume: () => void;
  onCancel: () => void;
  onFinalizeDocument: () => void;
  onResetSession: () => void;
  onEnterEditor: () => void;
  onChangeEditorText: (value: string) => void;
  onChangeEditorSlotText: (slotId: string, value: string) => void;
  onChangeEditorHasSelection: (value: boolean) => void;
  onSaveEditor: () => void;
  onSaveEditorAndExit: () => void;
  onDiscardEditorChanges: () => void;
  onExitEditor: () => void;
  onToggleMarkers: () => void;
  onRewriteSelection: () => void;
}

export const DocumentPanel = memo(function DocumentPanel({
  settings,
  settingsReady,
  currentSession,
  currentStats,
  showMarkers,
  suggestionsByRewriteUnit,
  runningRewriteUnitIdSet,
  optimisticManualRunningRewriteUnitId,
  activeRewriteUnitId,
  activeSuggestionId,
  activeReviewNavigationRequestId,
  selectedRewriteUnitIds,
  busyAction,
  editorMode,
  editorText,
  editorSlotOverrides,
  editorDirty,
  editorHasSelection,
  editorRef,
  documentScrollRef,
  onOpenDocument,
  onOpenSettings,
  onSelectRewriteUnit,
  onSelectSuggestion,
  onStartRewrite,
  onPause,
  onResume,
  onCancel,
  onFinalizeDocument,
  onResetSession,
  onEnterEditor,
  onChangeEditorText,
  onChangeEditorSlotText,
  onChangeEditorHasSelection,
  onSaveEditor,
  onSaveEditorAndExit,
  onDiscardEditorChanges,
  onExitEditor,
  onToggleMarkers,
  onRewriteSelection
}: DocumentPanelProps) {
  const [documentView, setDocumentView] = useState<DocumentView>("markup");

  const rewriteRunning = currentSession?.status === "running";
  const rewritePaused = currentSession?.status === "paused";
  const currentDocumentBackend = currentSession ? documentBackendKind(currentSession) : null;
  const currentEditorMode = currentSession ? documentEditorMode(currentSession) : "none";
  const sourceWritebackSupported = currentSession ? sessionSupportsSourceWriteback(currentSession) : false;
  const readOnlyDocument = currentDocumentBackend === "pdf";
  const anyBusy = Boolean(busyAction);

  const startKey = `start-${settings.rewriteMode}`;
  const startBusy = busyAction === startKey;
  const pauseBusy = busyAction === "pause-rewrite";
  const resumeBusy = busyAction === "resume-rewrite";
  const cancelBusy = busyAction === "cancel-rewrite";
  const finalizeBusy = busyAction === "finalize-document";
  const resetBusy = busyAction === "reset-session";
  const saveAndExitBusy = busyAction === "save-edits-and-back";
  const rewriteSelectionBusy = busyAction === "rewrite-selection";

  const showCancelAction = rewriteRunning || rewritePaused;
  const hasAppliedEdits = Boolean(currentStats && currentStats.suggestionsApplied > 0);
  const hasRewriteUnitSelection = hasSelectedRewriteUnits(selectedRewriteUnitIds);
  const effectiveSegmentationPreset = currentSession?.segmentationPreset ?? settings.segmentationPreset;
  const selectedDisplayCount = countSelectedRewriteUnits(selectedRewriteUnitIds);
  const rewriteBlockReason = rewriteBlockedReason(currentSession);
  const nextManualTargetRewriteUnit = useMemo(
    () =>
      currentSession
        ? findNextManualTargetRewriteUnit(currentSession, selectedRewriteUnitIds)
        : null,
    [currentSession, selectedRewriteUnitIds]
  );
  const autoPendingTargetRewriteUnits = useMemo(
    () =>
      currentSession
        ? findAutoPendingTargetRewriteUnits(currentSession, selectedRewriteUnitIds)
        : [],
    [currentSession, selectedRewriteUnitIds]
  );

  const canStartRewrite = Boolean(
    settingsReady &&
      currentSession &&
      canRewriteSession(currentSession) &&
      !rewriteRunning &&
      !rewritePaused &&
      (settings.rewriteMode === "manual"
        ? nextManualTargetRewriteUnit
        : autoPendingTargetRewriteUnits.length > 0)
  );

  const runKey = rewriteRunning
    ? "pause-rewrite"
    : rewritePaused
      ? "resume-rewrite"
      : startKey;
  const runBusy = rewriteRunning ? pauseBusy : rewritePaused ? resumeBusy : startBusy;

  const runLabel = useMemo(() => {
    if (rewriteRunning) return "暂停";
    if (rewritePaused) return "继续";
    if (hasRewriteUnitSelection) return "处理所选";
    return settings.rewriteMode === "auto" ? "开始批处理" : "开始优化";
  }, [hasRewriteUnitSelection, rewritePaused, rewriteRunning, settings.rewriteMode]);

  const runTitle = useMemo(() => {
    if (rewriteRunning) return "暂停自动任务";
    if (rewritePaused) return "继续自动任务";
    if (!currentSession) return "请先打开一个文档";
    if (!settingsReady) return "请先在设置里配置 Base URL / Key / Model";
    if (rewriteBlockReason) return rewriteBlockReason;
    if (settings.rewriteMode === "manual" && !nextManualTargetRewriteUnit) {
      return hasRewriteUnitSelection ? "所选片段已处理完成" : "全部片段已生成，可在右侧处理后导出";
    }
    if (settings.rewriteMode === "auto" && autoPendingTargetRewriteUnits.length === 0) {
      return hasRewriteUnitSelection ? "所选片段已处理完成" : "全部片段已生成，可在右侧处理后导出";
    }
    if (hasRewriteUnitSelection) return `处理所选 ${selectedDisplayCount} 段`;
    return settings.rewriteMode === "auto" ? "自动批处理生成并应用" : "生成下一条建议";
  }, [
    autoPendingTargetRewriteUnits.length,
    currentSession,
    hasRewriteUnitSelection,
    nextManualTargetRewriteUnit,
    rewritePaused,
    rewriteRunning,
    rewriteBlockReason,
    selectedDisplayCount,
    settings.rewriteMode,
    settingsReady
  ]);

  const documentSubtitle = useMemo(
    () => (currentSession && editorMode ? "编辑终稿" : undefined),
    [currentSession, editorMode]
  );

  const canEnterEditor = Boolean(
    currentSession &&
      currentSession.capabilities.editorEntry.allowed &&
      !rewriteRunning &&
      !rewritePaused &&
      !anyBusy
  );

  const enterEditorTitle = useMemo(() => {
    if (!currentSession) return "请先打开一个文档";
    if (!currentSession.capabilities.editorEntry.allowed) {
      return editorEntryBlockedReason(currentSession) ?? "当前文档暂不支持进入编辑模式。";
    }
    if (rewriteRunning || rewritePaused) {
      return "请先取消自动任务后再编辑终稿";
    }
    if (anyBusy) return "当前有操作在执行，请稍后再试";
    if (!sessionIsClean(currentSession)) {
      return currentSession.capabilities.editorEntry.blockReason ?? "当前文档暂不支持进入编辑模式。";
    }
    return "编辑终稿（仅在无修订记录时开放）";
  }, [anyBusy, currentSession, rewritePaused, rewriteRunning]);

  const finalizeDisabled =
    editorMode ||
    finalizeBusy ||
    (anyBusy && busyAction !== "finalize-document") ||
    rewriteRunning ||
    rewritePaused ||
    !hasAppliedEdits ||
    !sourceWritebackSupported;

  const finalizeTitle = useMemo(() => {
    if (finalizeBusy) return "正在写回原文件…";
    if (currentSession && !sourceWritebackSupported) {
      return currentSession.capabilities.sourceWriteback.blockReason ?? "当前文档暂不支持写回覆盖。";
    }
    if (rewriteRunning || rewritePaused) {
      return "请先取消自动任务后再写回原文件";
    }
    if (!currentStats) return "正在计算会话状态…";
    if (currentStats.suggestionsApplied === 0) {
      return "还没有已应用的修改（先在右侧点“应用”）";
    }
    return "覆盖原文件并清理记录（不可撤销）";
  }, [
    currentSession,
    currentStats,
    finalizeBusy,
    rewritePaused,
    rewriteRunning,
    sourceWritebackSupported
  ]);

  const { canCopy, copyState, copyTitle, handleCopyDocument } = useCopyDocument({
    editorMode,
    editorText,
    documentView,
    currentSession
  });

  const documentFormat = useMemo(
    () => guessClientDocumentFormat(currentSession?.documentPath ?? ""),
    [currentSession?.documentPath]
  );

  const editorCharacterCount = useMemo(
    () => (editorMode ? countCharacters(editorText) : 0),
    [editorMode, editorText]
  );

  const resetDisabled =
    editorMode ||
    !currentSession ||
    rewriteRunning ||
    rewritePaused ||
    resetBusy ||
    (anyBusy && busyAction !== "reset-session");

  const cancelDisabled =
    editorMode ||
    !showCancelAction ||
    cancelBusy ||
    (anyBusy && busyAction !== "cancel-rewrite");

  const runDisabled =
    editorMode ||
    (rewriteRunning
      ? pauseBusy || (anyBusy && busyAction !== runKey)
      : rewritePaused
        ? resumeBusy || (anyBusy && busyAction !== runKey)
        : !canStartRewrite || startBusy || (anyBusy && busyAction !== runKey));

  const discardVisible = editorDirty;
  const discardDisabled = !editorDirty || anyBusy;
  const discardTitle = anyBusy ? "当前有操作在执行，请稍后再试" : "放弃未保存修改";

  const editorPrimaryTitle = editorDirty
    ? saveAndExitBusy
      ? "正在写回原文件…"
      : anyBusy
        ? "当前有操作在执行，请稍后再试"
        : "保存并返回工作台"
    : anyBusy
      ? "当前有操作在执行，请稍后再试"
      : "返回工作台";

  const editorPrimaryDisabled = editorDirty
    ? saveAndExitBusy || (anyBusy && !saveAndExitBusy)
    : anyBusy;

  const canRewriteSelection = editorHasSelection;
  const rewriteSelectionDisabled = !editorMode || !canRewriteSelection || anyBusy;
  const rewriteSelectionTitle = !editorMode
    ? "仅在编辑终稿中可用"
    : anyBusy
      ? "当前有操作在执行，请稍后再试"
      : canRewriteSelection
        ? "对当前选区执行降 AIGC 处理"
        : "请先在正文中选中需要处理的文本";

  const handleCopy = useCallback(() => {
    void handleCopyDocument();
  }, [handleCopyDocument]);

  return (
    <Panel
      title="文档"
      subtitle={documentSubtitle}
      className="workbench-doc-panel"
      bodyClassName="workbench-center-body"
      action={
        currentSession ? (
          <DocumentActionBar
            editorMode={editorMode}
            documentView={documentView}
            onSetDocumentView={setDocumentView}
            showMarkers={showMarkers}
            onToggleMarkers={onToggleMarkers}
            canCopy={canCopy}
            copyState={copyState}
            copyTitle={copyTitle}
            onCopy={handleCopy}
            editorDirty={editorDirty}
            editorCharacterCount={editorCharacterCount}
            canEnterEditor={canEnterEditor}
            enterEditorTitle={enterEditorTitle}
            onEnterEditor={onEnterEditor}
            resetBusy={resetBusy}
            resetDisabled={resetDisabled}
            onResetSession={onResetSession}
            hasAppliedEdits={hasAppliedEdits}
            finalizeBusy={finalizeBusy}
            finalizeDisabled={finalizeDisabled}
            finalizeTitle={finalizeTitle}
            onFinalizeDocument={onFinalizeDocument}
            showCancelAction={showCancelAction}
            cancelBusy={cancelBusy}
            cancelDisabled={cancelDisabled}
            onCancel={onCancel}
            rewriteRunning={rewriteRunning ?? false}
            rewritePaused={rewritePaused ?? false}
            rewriteMode={settings.rewriteMode}
            runBusy={runBusy}
            runDisabled={runDisabled}
            runTitle={runTitle}
            runLabel={runLabel}
            onStartRewrite={onStartRewrite}
            onPause={onPause}
            onResume={onResume}
            discardVisible={discardVisible}
            discardDisabled={discardDisabled}
            discardTitle={discardTitle}
            onDiscardEditorChanges={onDiscardEditorChanges}
            editorPrimaryBusy={saveAndExitBusy}
            editorPrimaryDisabled={editorPrimaryDisabled}
            editorPrimaryTitle={editorPrimaryTitle}
            onSaveEditorAndExit={onSaveEditorAndExit}
            onExitEditor={onExitEditor}
            rewriteSelectionBusy={rewriteSelectionBusy}
            rewriteSelectionDisabled={rewriteSelectionDisabled}
            rewriteSelectionTitle={rewriteSelectionTitle}
            onRewriteSelection={onRewriteSelection}
          />
        ) : null
      }
    >
      {currentSession ? (
        <article className="editor-paper workbench-editor-paper">
          <div ref={documentScrollRef} className="paper-content scroll-region">
            {editorMode ? (
              <DocumentEditor
                ref={editorRef}
                session={currentSession}
                value={editorText}
                slotOverrides={editorSlotOverrides}
                dirty={editorDirty}
                busy={anyBusy}
                onChange={onChangeEditorText}
                onChangeSlotText={onChangeEditorSlotText}
                onSave={onSaveEditor}
                onSelectionChange={onChangeEditorHasSelection}
              />
            ) : (
              <DocumentFlow
                sessionId={currentSession.id}
                session={currentSession}
                rewriteUnits={currentSession.rewriteUnits}
                segmentationPreset={effectiveSegmentationPreset}
                documentView={documentView}
                documentFormat={documentFormat}
                rewriteEnabled={!rewriteBlockReason}
                rewriteBlockedReason={rewriteBlockReason}
                showMarkers={showMarkers}
                suggestionsByRewriteUnit={suggestionsByRewriteUnit}
                runningRewriteUnitIdSet={runningRewriteUnitIdSet}
                optimisticManualRunningRewriteUnitId={optimisticManualRunningRewriteUnitId}
                activeRewriteUnitId={activeRewriteUnitId}
                activeSuggestionId={activeSuggestionId}
                activeReviewNavigationRequestId={activeReviewNavigationRequestId}
                selectedRewriteUnitIds={selectedRewriteUnitIds}
                onSelectRewriteUnit={onSelectRewriteUnit}
                onSelectSuggestion={onSelectSuggestion}
              />
            )}
          </div>
        </article>
      ) : (
        <DocumentEmptyState
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
