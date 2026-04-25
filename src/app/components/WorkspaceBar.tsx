import { memo, type PointerEvent as ReactPointerEvent } from "react";
import {
  Copy,
  Download,
  FolderOpen,
  LoaderCircle,
  Minus,
  Settings2,
  Square,
  X
} from "lucide-react";
import type { AppSettings, DocumentSession, RewriteProgress } from "../../lib/types";
import {
  formatDisplayPath,
  formatSessionStatus,
  statusTone
} from "../../lib/helpers";
import { isWindowDragExcludedTarget } from "../../lib/windowDrag";
import { StatusBadge } from "../../components/StatusBadge";

interface WorkspaceBarProps {
  logoUrl: string;
  stage: "workbench" | "editor";
  settingsOpen: boolean;
  settingsReady: boolean;
  settings: AppSettings;
  currentSession: DocumentSession | null;
  topbarProgress: string;
  liveProgress: RewriteProgress | null;
  busyAction: string | null;
  windowMaximized: boolean;
  onOpenDocument: () => void;
  onOpenSettings: () => void;
  onExport: () => void;
  onStartWindowDrag: () => void;
  onMinimizeWindow: () => void;
  onToggleMaximizeWindow: () => void;
  onCloseWindow: () => void;
}

export const WorkspaceBar = memo(function WorkspaceBar({
  logoUrl,
  stage,
  settingsOpen,
  settingsReady,
  settings,
  currentSession,
  topbarProgress,
  liveProgress,
  busyAction,
  windowMaximized,
  onOpenDocument,
  onOpenSettings,
  onExport,
  onStartWindowDrag,
  onMinimizeWindow,
  onToggleMaximizeWindow,
  onCloseWindow
}: WorkspaceBarProps) {
  const openDisabled =
    stage === "editor" ||
    Boolean(busyAction) ||
    Boolean(currentSession && ["running", "paused"].includes(currentSession.status));

  const exportDisabled =
    stage === "editor" ||
    !currentSession ||
    Boolean(busyAction) ||
    Boolean(currentSession && ["running", "paused"].includes(currentSession.status));

  const rawPath = currentSession ? formatDisplayPath(currentSession.documentPath) : "";
  const handleHeaderPointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.button !== 0 || !event.isPrimary) {
      return;
    }

    if (isWindowDragExcludedTarget(event.target)) {
      return;
    }

    event.preventDefault();
    event.stopPropagation();
    void onStartWindowDrag();
  };

  return (
    <div
      className="workspace-bar"
      onPointerDown={handleHeaderPointerDown}
      onDoubleClick={(event) => event.preventDefault()}
    >
      <div className="workspace-bar-left">
        <img className="brand-logo is-small" src={logoUrl} alt="LessAI" draggable={false} />
        <div className="workspace-bar-brand">
          <strong>LessAI</strong>
          <span className="workspace-bar-view">
            {settingsOpen ? "Settings" : stage === "editor" ? "Editor" : "Workbench"}
          </span>
        </div>
      </div>

      <div className="workspace-bar-center">
        <div className="workspace-bar-status-row">
          <div
            className="workspace-bar-chips scroll-region"
            data-tauri-drag-region="false"
            data-window-drag-exclude="true"
          >
            <StatusBadge
              tone={
                currentSession ? statusTone(currentSession.status) : settingsReady ? "info" : "warning"
              }
            >
              {currentSession
                ? formatSessionStatus(currentSession.status)
                : settingsReady
                  ? "未打开"
                  : "未配置"}
            </StatusBadge>
            <span className="context-chip">模型：{settings.model}</span>
            <span className="context-chip">应用：{topbarProgress}</span>
            {liveProgress && currentSession && liveProgress.sessionId === currentSession.id ? (
              <span className="context-chip">
                进度 {liveProgress.completedUnits}/{liveProgress.totalUnits}
                {liveProgress.inFlight > 0 ? ` · 进行中 ${liveProgress.inFlight}` : ""}
                {liveProgress.maxConcurrency > 1 ? ` · 并发 ${liveProgress.maxConcurrency}` : ""}
              </span>
            ) : null}
          </div>
        </div>
        {currentSession ? (
          <div className="workspace-bar-path-line" title={`路径：${rawPath}`}>
            <span className="workspace-bar-path-label">路径：</span>
            <span className="workspace-bar-path-text">{rawPath}</span>
          </div>
        ) : null}
      </div>

      <div
        className="workspace-bar-actions"
        data-tauri-drag-region="false"
        data-window-drag-exclude="true"
      >
        <button
          type="button"
          className="icon-button"
          onClick={onOpenDocument}
          aria-label="打开文档"
          title="打开文件"
          disabled={openDisabled}
        >
          {busyAction === "open-document" ? <LoaderCircle className="spin" /> : <FolderOpen />}
        </button>

        <button
          type="button"
          className="icon-button"
          onClick={onOpenSettings}
          aria-label="打开设置"
          title="设置"
        >
          <Settings2 />
        </button>
        <button
          type="button"
          className="icon-button"
          onClick={onExport}
          aria-label="导出终稿"
          title="导出"
          disabled={exportDisabled}
        >
          {busyAction === "export-document" ? <LoaderCircle className="spin" /> : <Download />}
        </button>
      </div>

      <div
        className="window-controls"
        data-tauri-drag-region="false"
        data-window-drag-exclude="true"
      >
        <button
          type="button"
          className="window-control-button"
          onClick={onMinimizeWindow}
          aria-label="最小化窗口"
          title="最小化"
        >
          <Minus />
        </button>
        <button
          type="button"
          className="window-control-button"
          onClick={onToggleMaximizeWindow}
          aria-label={windowMaximized ? "还原窗口" : "最大化窗口"}
          title={windowMaximized ? "还原" : "最大化"}
        >
          {windowMaximized ? <Copy /> : <Square />}
        </button>
        <button
          type="button"
          className="window-control-button is-close"
          onClick={onCloseWindow}
          aria-label="关闭窗口"
          title="关闭"
        >
          <X />
        </button>
      </div>
    </div>
  );
});
