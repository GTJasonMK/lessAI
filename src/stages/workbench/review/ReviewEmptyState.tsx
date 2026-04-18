import { memo } from "react";
import { FileDiff, Settings2 } from "lucide-react";
import { ActionButton } from "../../../components/ActionButton";

interface ReviewEmptyStateProps {
  settingsReady: boolean;
  onOpenSettings: () => void;
}

export const ReviewEmptyState = memo(function ReviewEmptyState({
  settingsReady,
  onOpenSettings
}: ReviewEmptyStateProps) {
  return (
    <div className="empty-state">
      <FileDiff />
      <div>
        <strong>这里会展示建议与候选稿</strong>
        <span>先打开一个文档，然后点击左侧文档右上角的“开始优化”。</span>
      </div>
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
