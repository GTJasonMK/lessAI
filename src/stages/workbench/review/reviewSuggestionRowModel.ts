import type { RewriteSuggestion, SuggestionDecision } from "../../../lib/types";

interface ReviewSuggestionRowActionStateInput {
  suggestionId: string;
  decision: SuggestionDecision;
  busyAction: string | null;
  anyBusy: boolean;
  editorMode: boolean;
  rewriteRunning: boolean;
  rewritePaused: boolean;
  settingsReady: boolean;
  rewriteUnitFailed: boolean;
}

export interface ReviewSuggestionRowActionState {
  applyBusy: boolean;
  applyDisabled: boolean;
  deleteBusy: boolean;
  deleteDisabled: boolean;
  dismissBusy: boolean;
  dismissDisabled: boolean;
  retryBusy: boolean;
  retryDisabled: boolean;
  retryVisible: boolean;
}

function compactWhitespace(value: string) {
  return value.replace(/\s+/g, " ").trim();
}

function ellipsis(value: string, maxChars: number) {
  return value.length > maxChars ? `${value.slice(0, maxChars)}…` : value;
}

export function buildSuggestionRowTitle(suggestion: RewriteSuggestion, maxChars = 32) {
  const preferred =
    compactWhitespace(suggestion.afterText) ||
    compactWhitespace(suggestion.beforeText) ||
    "（空片段）";
  return `#${suggestion.sequence} ${ellipsis(preferred, maxChars)}`;
}

export function buildSuggestionRowPrimaryActionLabel(decision: SuggestionDecision) {
  if (decision === "applied") {
    return "忽略";
  }
  return "应用";
}

export function buildSuggestionRowActionState(
  input: ReviewSuggestionRowActionStateInput
): ReviewSuggestionRowActionState {
  const sharedBlocked = input.editorMode || input.rewriteRunning || input.rewritePaused;
  const applyBusy = input.busyAction === `apply-suggestion:${input.suggestionId}`;
  const deleteBusy = input.busyAction === `delete-suggestion:${input.suggestionId}`;
  const dismissBusy = input.busyAction === `dismiss-suggestion:${input.suggestionId}`;
  const retryBusy = input.busyAction === "retry-rewrite-unit";

  return {
    applyBusy,
    applyDisabled:
      sharedBlocked ||
      input.decision === "applied" ||
      applyBusy ||
      (input.anyBusy && !applyBusy),
    deleteBusy,
    deleteDisabled: sharedBlocked || deleteBusy || (input.anyBusy && !deleteBusy),
    dismissBusy,
    dismissDisabled:
      sharedBlocked ||
      input.decision === "dismissed" ||
      dismissBusy ||
      (input.anyBusy && !dismissBusy),
    retryBusy,
    retryDisabled:
      !input.rewriteUnitFailed ||
      !input.settingsReady ||
      sharedBlocked ||
      retryBusy ||
      (input.anyBusy && !retryBusy),
    retryVisible: input.rewriteUnitFailed
  };
}
