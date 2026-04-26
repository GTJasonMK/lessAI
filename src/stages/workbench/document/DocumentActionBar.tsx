import { memo } from "react";
import {
  AlertCircle,
  ArrowLeft,
  Check,
  Copy,
  FileCheck2,
  FilePenLine,
  LoaderCircle,
  Pause,
  Play,
  RotateCcw,
  Square,
  Undo2,
  WandSparkles
} from "lucide-react";
import type { RewriteMode } from "../../../lib/types";
import type { DocumentView } from "../hooks/useCopyDocument";

const DOCUMENT_VIEW_OPTIONS: ReadonlyArray<{
  key: DocumentView;
  label: string;
  hint: string;
}> = [
  { key: "markup", label: "修订标记", hint: "查看插入/删除标记与高亮" },
  { key: "final", label: "修改后", hint: "按当前最新候选合并成整篇" },
  { key: "source", label: "修改前", hint: "查看原文整篇（不含任何改写）" }
];

type CopyState = "idle" | "copying" | "done" | "error";

interface DocumentActionBarProps {
  editorMode: boolean;
  documentView: DocumentView;
  onSetDocumentView: (view: DocumentView) => void;
  showMarkers: boolean;
  onToggleMarkers: () => void;

  canCopy: boolean;
  copyState: CopyState;
  copyTitle: string;
  onCopy: () => void;

  editorDirty: boolean;
  canEnterEditor: boolean;
  enterEditorTitle: string;
  onEnterEditor: () => void;

  resetBusy: boolean;
  resetDisabled: boolean;
  onResetSession: () => void;

  hasAppliedEdits: boolean;
  finalizeBusy: boolean;
  finalizeDisabled: boolean;
  finalizeTitle: string;
  onFinalizeDocument: () => void;

  showCancelAction: boolean;
  cancelBusy: boolean;
  cancelDisabled: boolean;
  onCancel: () => void;

  rewriteRunning: boolean;
  rewritePaused: boolean;
  rewriteMode: RewriteMode;
  runBusy: boolean;
  runDisabled: boolean;
  runTitle: string;
  runLabel: string;
  onStartRewrite: (mode: RewriteMode) => void;
  onPause: () => void;
  onResume: () => void;

  discardDisabled: boolean;
  discardTitle: string;
  onDiscardEditorChanges: () => void;

  editorPrimaryBusy: boolean;
  editorPrimaryDisabled: boolean;
  editorPrimaryTitle: string;
  onSaveEditorAndExit: () => void;
  onExitEditor: () => void;

  rewriteSelectionBusy: boolean;
  rewriteSelectionDisabled: boolean;
  rewriteSelectionTitle: string;
  onRewriteSelection: () => void;
}

export const DocumentActionBar = memo(function DocumentActionBar({
  editorMode,
  documentView,
  onSetDocumentView,
  showMarkers,
  onToggleMarkers,
  canCopy,
  copyState,
  copyTitle,
  onCopy,
  editorDirty,
  canEnterEditor,
  enterEditorTitle,
  onEnterEditor,
  resetBusy,
  resetDisabled,
  onResetSession,
  hasAppliedEdits,
  finalizeBusy,
  finalizeDisabled,
  finalizeTitle,
  onFinalizeDocument,
  showCancelAction,
  cancelBusy,
  cancelDisabled,
  onCancel,
  rewriteRunning,
  rewritePaused,
  rewriteMode,
  runBusy,
  runDisabled,
  runTitle,
  runLabel,
  onStartRewrite,
  onPause,
  onResume,
  discardDisabled,
  discardTitle,
  onDiscardEditorChanges,
  editorPrimaryBusy,
  editorPrimaryDisabled,
  editorPrimaryTitle,
  onSaveEditorAndExit,
  onExitEditor,
  rewriteSelectionBusy,
  rewriteSelectionDisabled,
  rewriteSelectionTitle,
  onRewriteSelection
}: DocumentActionBarProps) {
  const copyIcon = !canCopy ? (
    <Copy />
  ) : copyState === "copying" ? (
    <LoaderCircle className="spin" />
  ) : copyState === "done" ? (
    <Check />
  ) : copyState === "error" ? (
    <AlertCircle />
  ) : (
    <Copy />
  );

  return (
    <div className="workbench-doc-actionbar">
      <div
        className="workbench-doc-actionbar-left"
        aria-label="文档视图与编辑状态"
      >
        <div
          className={`workbench-action-reel workbench-view-reel ${editorMode ? "is-editor" : ""}`}
        >
          <div className="workbench-action-track">
            <div className="workbench-action-row is-normal" aria-hidden={editorMode}>
              {DOCUMENT_VIEW_OPTIONS.map((option) => (
                <button
                  key={option.key}
                  type="button"
                  className={`switch-chip ${documentView === option.key ? "is-active" : ""}`}
                  onClick={() => onSetDocumentView(option.key)}
                  aria-label={`切换到${option.label}视图`}
                  title={option.hint}
                  disabled={editorMode}
                >
                  {option.label}
                </button>
              ))}
            </div>

            <div className="workbench-action-row is-editor" aria-hidden={!editorMode} />
          </div>
        </div>
      </div>

      <button
        type="button"
        className={`switch-chip workbench-doc-markers-toggle ${showMarkers ? "is-active" : ""}`}
        onClick={onToggleMarkers}
        aria-label={showMarkers ? "隐藏辅助标记" : "显示辅助标记"}
        title={
          showMarkers
            ? "隐藏编辑范围/保护区等辅助高亮（更适合通读）"
            : "显示编辑范围/保护区等辅助高亮（更适合精修）"
        }
      >
        {showMarkers ? "标记：开" : "标记：关"}
      </button>

      <div
        className={`workbench-doc-actionbar-right ${editorMode ? "is-editor-mode" : "is-normal-mode"}`}
      >
        <div
          className={`workbench-action-reel workbench-doc-actionbar-right-reel ${editorMode ? "is-editor" : ""}`}
        >
          <div className="workbench-action-track">
            <div className="workbench-action-row is-normal" aria-hidden={editorMode}>
              <button
                type="button"
                className="icon-button"
                onClick={onEnterEditor}
                aria-label="进入编辑模式"
                title={enterEditorTitle}
                disabled={!canEnterEditor}
              >
                <FilePenLine />
              </button>

              <button
                type="button"
                className="icon-button"
                onClick={onResetSession}
                aria-label="重置该文档记录（不修改原文件）"
                title="重置该文档记录（不修改原文件）"
                disabled={resetDisabled}
              >
                {resetBusy ? <LoaderCircle className="spin" /> : <RotateCcw />}
              </button>

              <button
                type="button"
                className={`icon-button ${hasAppliedEdits ? "is-danger" : ""}`}
                onClick={onFinalizeDocument}
                aria-label="覆盖原文件并清理记录"
                title={finalizeTitle}
                disabled={finalizeDisabled}
              >
                {finalizeBusy ? <LoaderCircle className="spin" /> : <FileCheck2 />}
              </button>

              <button
                type="button"
                className={`icon-button ${showCancelAction ? "" : "is-placeholder"}`}
                onClick={onCancel}
                aria-label="取消执行"
                title="取消"
                aria-hidden={!showCancelAction}
                tabIndex={showCancelAction ? 0 : -1}
                disabled={cancelDisabled}
              >
                {cancelBusy ? <LoaderCircle className="spin" /> : <Square />}
              </button>

              <button
                type="button"
                className={`toolbar-button is-run-action workbench-doc-primary-action ${
                  rewriteRunning ? "is-warning" : "is-primary"
                }`}
                onClick={() => {
                  if (rewriteRunning) {
                    onPause();
                    return;
                  }
                  if (rewritePaused) {
                    onResume();
                    return;
                  }
                  onStartRewrite(rewriteMode);
                }}
                aria-label={
                  rewriteRunning
                    ? "暂停执行"
                    : rewritePaused
                      ? "继续执行"
                      : rewriteMode === "auto"
                        ? "开始批处理"
                        : "开始优化"
                }
                title={runTitle}
                disabled={runDisabled}
              >
                {runBusy ? (
                  <LoaderCircle className="spin" />
                ) : rewriteRunning ? (
                  <Pause />
                ) : rewritePaused ? (
                  <Play />
                ) : (
                  <WandSparkles />
                )}
                <span>{runLabel}</span>
              </button>

              <span className="icon-button is-placeholder" aria-hidden="true" />

              <button
                type="button"
                className="icon-button"
                onClick={onCopy}
                aria-label={canCopy ? copyTitle : "复制（当前视图不可用）"}
                title={copyTitle}
                disabled={!canCopy || copyState === "copying"}
              >
                {copyIcon}
              </button>
            </div>

            <div className="workbench-action-row is-editor" aria-hidden={!editorMode}>
              <span
                className="icon-button is-placeholder workbench-doc-fixed-placeholder"
                aria-hidden="true"
              />
              <span
                className="icon-button is-placeholder workbench-doc-fixed-placeholder"
                aria-hidden="true"
              />
              <span
                className="icon-button is-placeholder workbench-doc-fixed-placeholder"
                aria-hidden="true"
              />

              <button
                type="button"
                className="icon-button is-danger"
                onClick={onDiscardEditorChanges}
                aria-label="放弃未保存修改"
                title={discardTitle}
                disabled={discardDisabled}
              >
                <Undo2 />
              </button>

              <button
                type="button"
                className="toolbar-button is-run-action workbench-doc-primary-action is-primary"
                onClick={() => {
                  if (editorDirty) {
                    onSaveEditorAndExit();
                    return;
                  }
                  onExitEditor();
                }}
                aria-label={editorDirty ? "保存并退出编辑模式" : "返回工作台"}
                title={editorPrimaryTitle}
                disabled={editorPrimaryDisabled}
              >
                {editorDirty ? (
                  editorPrimaryBusy ? (
                    <LoaderCircle className="spin" />
                  ) : (
                    <Check />
                  )
                ) : (
                  <ArrowLeft />
                )}
                <span>{editorDirty ? "保存并退出" : "返回工作台"}</span>
              </button>

              <button
                type="button"
                className="icon-button"
                onClick={onRewriteSelection}
                aria-label="对选区执行降 AIGC 处理"
                title={rewriteSelectionTitle}
                disabled={rewriteSelectionDisabled}
              >
                {rewriteSelectionBusy ? <LoaderCircle className="spin" /> : <WandSparkles />}
              </button>

              <button
                type="button"
                className="icon-button"
                onClick={onCopy}
                aria-label={canCopy ? copyTitle : "复制（当前视图不可用）"}
                title={copyTitle}
                disabled={!canCopy || copyState === "copying"}
              >
                {copyIcon}
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
});
