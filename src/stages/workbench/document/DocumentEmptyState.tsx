import { memo } from "react";
import { FolderOpen, Settings2 } from "lucide-react";
import { ActionButton } from "../../../components/ActionButton";

interface DocumentEmptyStateProps {
  busyAction: string | null;
  anyBusy: boolean;
  settingsReady: boolean;
  onOpenDocument: () => void;
  onOpenSettings: () => void;
}

export const DocumentEmptyState = memo(function DocumentEmptyState({
  busyAction,
  anyBusy,
  settingsReady,
  onOpenDocument,
  onOpenSettings
}: DocumentEmptyStateProps) {
  return (
    <div className="empty-state">
      <FolderOpen />
      <div>
        <strong>打开一个文档开始</strong>
        <span>LessAI 会为该文件保存优化进度，下次打开同一文件会自动恢复。</span>
      </div>
      <ActionButton
        icon={FolderOpen}
        label="打开文件"
        busy={busyAction === "open-document"}
        disabled={anyBusy && busyAction !== "open-document"}
        onClick={onOpenDocument}
        variant="primary"
      />
      {!settingsReady ? (
        <ActionButton
          icon={Settings2}
          label="先去设置接口与模型"
          busy={false}
          onClick={onOpenSettings}
          variant="secondary"
        />
      ) : null}
    </div>
  );
});

