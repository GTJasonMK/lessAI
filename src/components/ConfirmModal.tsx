import { memo, useEffect } from "react";
import { X } from "lucide-react";

export type ConfirmVariant = "primary" | "danger";

export interface ConfirmModalOptions {
  title: string;
  message: string;
  okLabel?: string;
  cancelLabel?: string;
  variant?: ConfirmVariant;
}

interface ConfirmModalProps extends ConfirmModalOptions {
  open: boolean;
  onResult: (value: boolean) => void;
}

export const ConfirmModal = memo(function ConfirmModal({
  open,
  title,
  message,
  okLabel = "确认",
  cancelLabel = "取消",
  variant = "primary",
  onResult
}: ConfirmModalProps) {
  useEffect(() => {
    if (!open) return;
    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        onResult(false);
      }
      if (event.key === "Enter") {
        onResult(true);
      }
    }

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [open, onResult]);

  if (!open) return null;

  return (
    <div
      className="modal-overlay"
      data-window-drag-exclude="true"
      role="dialog"
      aria-modal="true"
      aria-label={title}
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) {
          onResult(false);
        }
      }}
    >
      <div className="dialog-card">
        <header className="dialog-header">
          <div className="dialog-header-title">
            <h2>{title}</h2>
          </div>
          <button
            type="button"
            className="icon-button"
            onClick={() => onResult(false)}
            aria-label="关闭"
            title="关闭"
          >
            <X />
          </button>
        </header>

        <div className="dialog-body">
          <p className="dialog-message">{message}</p>
        </div>

        <footer className="dialog-footer">
          <div className="dialog-footer-actions">
            <button
              type="button"
              className="button button-secondary"
              onClick={() => onResult(false)}
            >
              {cancelLabel}
            </button>
            <button
              type="button"
              className={`button ${variant === "danger" ? "button-danger" : "button-primary"}`}
              onClick={() => onResult(true)}
            >
              {okLabel}
            </button>
          </div>
        </footer>
      </div>
    </div>
  );
});
