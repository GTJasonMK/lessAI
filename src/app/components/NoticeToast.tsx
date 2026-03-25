import { memo } from "react";
import { X } from "lucide-react";
import type { NoticeState } from "../../lib/constants";

interface NoticeToastProps {
  notice: NoticeState | null;
  onDismiss: () => void;
}

export const NoticeToast = memo(function NoticeToast({
  notice,
  onDismiss
}: NoticeToastProps) {
  if (!notice) return null;

  return (
    <div className="toast-layer" aria-live="polite" aria-label="操作提示">
      <div className={`notice is-${notice.tone} toast`}>
        <span>{notice.message}</span>
        <button
          type="button"
          className="notice-dismiss"
          onClick={onDismiss}
          aria-label="关闭提示"
          title="关闭"
        >
          <X />
        </button>
      </div>
    </div>
  );
});

