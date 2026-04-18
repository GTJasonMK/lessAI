import { memo } from "react";
import { LoaderCircle, RotateCcw } from "lucide-react";
import { formatDate } from "../../../lib/helpers";
import type { RewriteSuggestion } from "../../../lib/types";
import type { ReviewSuggestionRowActionState } from "./reviewSuggestionRowModel";
import {
  buildSuggestionRowPrimaryActionLabel,
  buildSuggestionRowTitle
} from "./reviewSuggestionRowModel";

interface ReviewSuggestionRowProps {
  suggestion: RewriteSuggestion;
  active: boolean;
  menuOpen: boolean;
  actionState: ReviewSuggestionRowActionState;
  onSelect: () => void;
  onApply: () => void;
  onDelete: () => void;
  onDismiss: () => void;
  onRetry: () => void;
  onToggleMenu: () => void;
}

export const ReviewSuggestionRow = memo(function ReviewSuggestionRow({
  suggestion,
  active,
  menuOpen,
  actionState,
  onSelect,
  onApply,
  onDelete,
  onDismiss,
  onRetry,
  onToggleMenu
}: ReviewSuggestionRowProps) {
  const showMenu = actionState.retryVisible;
  const primaryAction =
    suggestion.decision === "applied"
      ? {
          className: "review-suggestion-row-action is-dismiss",
          disabled: actionState.dismissDisabled,
          busy: actionState.dismissBusy,
          label: buildSuggestionRowPrimaryActionLabel(suggestion.decision),
          onClick: onDismiss,
          title: "忽略"
        }
      : {
          className: "review-suggestion-row-action is-apply",
          disabled: actionState.applyDisabled,
          busy: actionState.applyBusy,
          label: buildSuggestionRowPrimaryActionLabel(suggestion.decision),
          onClick: onApply,
          title: "应用"
        };
  const className = [
    "review-suggestion-row",
    `is-${suggestion.decision}`,
    active ? "is-active" : "",
    menuOpen ? "is-menu-open" : ""
  ]
    .filter(Boolean)
    .join(" ");

  return (
    <div className={className}>
      <button
        type="button"
        className="review-suggestion-row-main"
        onClick={onSelect}
        title={buildSuggestionRowTitle(suggestion, 80)}
      >
        <div className="review-suggestion-row-mainline">
          <span
            className="review-suggestion-row-state-dot"
            aria-hidden="true"
            title={suggestion.decision}
          />
          <span className="review-suggestion-row-title">
            {buildSuggestionRowTitle(suggestion, 72)}
          </span>
          <span className="review-suggestion-row-meta">{formatDate(suggestion.updatedAt)}</span>
        </div>
      </button>

      <div className="review-suggestion-row-actions">
        <button
          type="button"
          className={primaryAction.className}
          onClick={primaryAction.onClick}
          disabled={primaryAction.disabled}
          aria-label={primaryAction.title}
          title={primaryAction.title}
        >
          {primaryAction.busy ? <LoaderCircle className="spin" /> : null}
          <span>{primaryAction.label}</span>
        </button>
        <button
          type="button"
          className="review-suggestion-row-action is-delete"
          onClick={onDelete}
          disabled={actionState.deleteDisabled}
          aria-label="删除该建议"
          title="删除"
        >
          {actionState.deleteBusy ? <LoaderCircle className="spin" /> : null}
          <span>删除</span>
        </button>
        {showMenu ? (
          <button
            type="button"
            className="review-suggestion-row-action is-more"
            onClick={onToggleMenu}
            aria-label="更多操作"
            title="更多"
          >
            <span>···</span>
          </button>
        ) : null}
      </div>

      {menuOpen && showMenu ? (
        <div className="review-suggestion-row-menu">
          {actionState.retryVisible ? (
            <button
              type="button"
              className="review-suggestion-row-menu-item"
              onClick={onRetry}
              disabled={actionState.retryDisabled}
            >
              {actionState.retryBusy ? <LoaderCircle className="spin" /> : <RotateCcw />}
              <span>重试</span>
            </button>
          ) : null}
        </div>
      ) : null}
    </div>
  );
});
