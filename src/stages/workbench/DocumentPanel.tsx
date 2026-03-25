import { memo, useCallback, useMemo, useState } from "react";
import type {
  AppSettings,
  DocumentSession,
  EditSuggestion,
  RewriteMode
} from "../../lib/types";
import type { SessionStats } from "../../lib/helpers";
import { countCharacters, isDocxPath } from "../../lib/helpers";
import { Panel } from "../../components/Panel";
import { useCopyDocument, type DocumentView } from "./hooks/useCopyDocument";
import { DocumentActionBar } from "./document/DocumentActionBar";
import { DocumentEditor } from "./document/DocumentEditor";
import { DocumentEmptyState } from "./document/DocumentEmptyState";
import { DocumentFlow } from "./document/DocumentFlow";

interface DocumentPanelProps {
  settings: AppSettings;
  settingsReady: boolean;
  currentSession: DocumentSession | null;
  currentStats: SessionStats | null;
  suggestionsByChunk: Map<number, EditSuggestion[]>;
  runningIndexSet: Set<number>;
  optimisticManualRunningIndex: number | null;
  activeChunkIndex: number;
  busyAction: string | null;
  editorMode: boolean;
  editorText: string;
  editorDirty: boolean;
  onOpenDocument: () => void;
  onOpenSettings: () => void;
  onSelectChunk: (index: number) => void;
  onSelectSuggestion: (suggestionId: string) => void;
  onStartRewrite: (mode: RewriteMode) => void;
  onPause: () => void;
  onResume: () => void;
  onCancel: () => void;
  onFinalizeDocument: () => void;
  onResetSession: () => void;
  onEnterEditor: () => void;
  onChangeEditorText: (value: string) => void;
  onSaveEditor: () => void;
  onSaveEditorAndExit: () => void;
  onDiscardEditorChanges: () => void;
  onExitEditor: () => void;
}

export const DocumentPanel = memo(function DocumentPanel({
  settings,
  settingsReady,
  currentSession,
  currentStats,
  suggestionsByChunk,
  runningIndexSet,
  optimisticManualRunningIndex,
  activeChunkIndex,
  busyAction,
  editorMode,
  editorText,
  editorDirty,
  onOpenDocument,
  onOpenSettings,
  onSelectChunk,
  onSelectSuggestion,
  onStartRewrite,
  onPause,
  onResume,
  onCancel,
  onFinalizeDocument,
  onResetSession,
  onEnterEditor,
  onChangeEditorText,
  onSaveEditor,
  onSaveEditorAndExit,
  onDiscardEditorChanges,
  onExitEditor
}: DocumentPanelProps) {
  const [documentView, setDocumentView] = useState<DocumentView>("markup");

  const rewriteRunning = currentSession?.status === "running";
  const rewritePaused = currentSession?.status === "paused";
  const docxDocument = Boolean(
    currentSession && isDocxPath(currentSession.documentPath)
  );
  const anyBusy = Boolean(busyAction);

  const startKey = `start-${settings.rewriteMode}`;
  const startBusy = busyAction === startKey;
  const pauseBusy = busyAction === "pause-rewrite";
  const resumeBusy = busyAction === "resume-rewrite";
  const cancelBusy = busyAction === "cancel-rewrite";
  const finalizeBusy = busyAction === "finalize-document";
  const resetBusy = busyAction === "reset-session";
  const saveAndExitBusy = busyAction === "save-edits-and-back";

  const showCancelAction = rewriteRunning || rewritePaused;
  const hasAppliedEdits = Boolean(currentStats && currentStats.suggestionsApplied > 0);

  const canStartRewrite = Boolean(
    settingsReady &&
      currentSession &&
      currentStats &&
      !rewriteRunning &&
      !rewritePaused &&
      currentStats.pendingGeneration > 0
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
    return settings.rewriteMode === "auto" ? "开始批处理" : "开始优化";
  }, [rewritePaused, rewriteRunning, settings.rewriteMode]);

  const runTitle = useMemo(() => {
    if (rewriteRunning) return "暂停自动任务";
    if (rewritePaused) return "继续自动任务";
    if (!currentSession) return "请先打开一个文档";
    if (!settingsReady) return "请先在设置里配置 Base URL / Key / Model";
    if (!currentStats) return "正在计算会话状态…";
    if (currentStats.pendingGeneration === 0) {
      return "全部片段已生成，可在右侧审阅并导出";
    }
    return settings.rewriteMode === "auto" ? "自动批处理生成并应用" : "生成下一条修改对";
  }, [
    currentSession,
    currentStats,
    rewritePaused,
    rewriteRunning,
    settings.rewriteMode,
    settingsReady
  ]);

  const documentSubtitle = useMemo(() => {
    if (!currentSession) {
      return "导入文档后可切换：修改前 / 修改后 / 修订标记";
    }
    if (editorMode) {
      return "编辑终稿";
    }
    switch (documentView) {
      case "source":
        return "修改前（原文）";
      case "final":
        return "修改后（合并视图）";
      case "markup":
        return "含修订标记";
      default:
        return "文档";
    }
  }, [currentSession, documentView, editorMode]);

  const canEnterEditor = Boolean(
    currentSession &&
      !docxDocument &&
      !rewriteRunning &&
      !rewritePaused &&
      currentSession.status === "idle" &&
      currentSession.suggestions.length === 0 &&
      currentSession.chunks.every(
        (chunk) => chunk.status === "idle" || chunk.skipRewrite
      ) &&
      !anyBusy
  );

  const enterEditorTitle = useMemo(() => {
    if (!currentSession) return "请先打开一个文档";
    if (docxDocument) {
      return "docx 目前仅支持导入/改写/导出，暂不支持终稿编辑或写回覆盖";
    }
    if (rewriteRunning || rewritePaused) {
      return "请先取消自动任务后再编辑终稿";
    }
    if (anyBusy) return "当前有操作在执行，请稍后再试";
    if (currentSession.status !== "idle") {
      return "当前文档状态不是空闲，暂不可编辑终稿";
    }
    if (currentSession.suggestions.length > 0) {
      return "该文档存在修订记录，为避免冲突，请先“覆写并清理记录”或“重置记录”后再编辑";
    }
    if (currentSession.chunks.some((chunk) => !chunk.skipRewrite && chunk.status !== "idle")) {
      return "该文档存在生成进度/失败片段，为避免冲突，请先“覆写并清理记录”或“重置记录”后再编辑";
    }
    return "编辑终稿（仅在无修订记录时开放）";
  }, [anyBusy, currentSession, docxDocument, rewritePaused, rewriteRunning]);

  const finalizeDisabled =
    editorMode ||
    finalizeBusy ||
    (anyBusy && busyAction !== "finalize-document") ||
    rewriteRunning ||
    rewritePaused ||
    !hasAppliedEdits ||
    docxDocument;

  const finalizeTitle = useMemo(() => {
    if (finalizeBusy) return "正在写回原文件…";
    if (docxDocument) {
      return "docx 暂不支持写回覆盖，请导出为纯文本后再写回";
    }
    if (rewriteRunning || rewritePaused) {
      return "请先取消自动任务后再写回原文件";
    }
    if (!currentStats) return "正在计算会话状态…";
    if (currentStats.suggestionsApplied === 0) {
      return "还没有已应用的修改（先在右侧点“应用”）";
    }
    return "覆盖原文件并清理记录（不可撤销）";
  }, [currentStats, docxDocument, finalizeBusy, rewritePaused, rewriteRunning]);

  const { canCopy, copyState, copyTitle, handleCopyDocument } = useCopyDocument({
    editorMode,
    editorText,
    documentView,
    chunks: currentSession?.chunks ?? null,
    suggestionsByChunk
  });

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
          />
        ) : null
      }
    >
      {currentSession ? (
        <article className="editor-paper workbench-editor-paper">
          <div className="paper-content scroll-region">
            {editorMode ? (
              <DocumentEditor
                value={editorText}
                dirty={editorDirty}
                busy={anyBusy}
                onChange={onChangeEditorText}
                onSave={onSaveEditor}
              />
            ) : (
              <DocumentFlow
                sessionId={currentSession.id}
                chunks={currentSession.chunks}
                documentView={documentView}
                suggestionsByChunk={suggestionsByChunk}
                runningIndexSet={runningIndexSet}
                optimisticManualRunningIndex={optimisticManualRunningIndex}
                activeChunkIndex={activeChunkIndex}
                onSelectChunk={onSelectChunk}
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

