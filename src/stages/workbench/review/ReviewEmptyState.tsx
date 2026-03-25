import { memo } from "react";
import { FileDiff, FolderOpen, Settings2 } from "lucide-react";
import { ActionButton } from "../../../components/ActionButton";

interface ReviewEmptyStateProps {
  busyAction: string | null;
  anyBusy: boolean;
  settingsReady: boolean;
  onOpenDocument: () => void;
  onOpenSettings: () => void;
}

export const ReviewEmptyState = memo(function ReviewEmptyState({
  busyAction,
  anyBusy,
  settingsReady,
  onOpenDocument,
  onOpenSettings
}: ReviewEmptyStateProps) {
  return (
    <div className="empty-state">
      <FileDiff />
      <div>
        <strong>审阅区会展示 diff 与候选稿</strong>
        <span>先打开一个文档，然后点击左侧文档右上角的“开始优化”。</span>
      </div>
      <ActionButton
        icon={FolderOpen}
        label="打开文件"
        busy={busyAction === "open-document"}
        disabled={anyBusy && busyAction !== "open-document"}
        onClick={onOpenDocument}
        variant="secondary"
      />
      {!settingsReady ? (
        <ActionButton
          icon={Settings2}
          label="打开设置"
          busy={false}
          onClick={onOpenSettings}
          variant="primary"
        />
      ) : null}
    </div>
  );
});

