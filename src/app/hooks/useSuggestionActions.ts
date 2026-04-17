import { useCallback } from "react";
import { applySuggestion, deleteSuggestion, dismissSuggestion } from "../../lib/api";
import type { DocumentSession } from "../../lib/types";
import {
  canRewriteSession,
  findRewriteUnit,
  getLatestSuggestion,
  readableError
} from "../../lib/helpers";
import {
  resolveSelectionTargetRewriteUnitIds,
  toggleSelectedRewriteUnitIds
} from "../../lib/rewriteUnitSelection";
import {
  refreshSessionStateSilently,
  refreshRewriteableSessionOrNotify,
  runSessionActionOrNotify,
  type ApplySessionState,
  type RefreshSessionState,
  type ShowNotice,
  type WithBusy
} from "./sessionActionShared";

export function useSuggestionActions(options: {
  currentSessionRef: React.MutableRefObject<DocumentSession | null>;
  activeRewriteUnitIdRef: React.MutableRefObject<string | null>;
  captureDocumentScrollPosition: () => number | null;
  setActiveRewriteUnitId: React.Dispatch<React.SetStateAction<string | null>>;
  setActiveSuggestionId: React.Dispatch<React.SetStateAction<string | null>>;
  setSelectedRewriteUnitIds: React.Dispatch<React.SetStateAction<string[]>>;
  setReviewView: React.Dispatch<React.SetStateAction<"diff" | "source" | "candidate">>;
  applySessionState: ApplySessionState;
  refreshSessionState: RefreshSessionState;
  showNotice: ShowNotice;
  withBusy: WithBusy;
}) {
  const {
    currentSessionRef,
    activeRewriteUnitIdRef,
    captureDocumentScrollPosition,
    setActiveRewriteUnitId,
    setActiveSuggestionId,
    setSelectedRewriteUnitIds,
    setReviewView,
    applySessionState,
    refreshSessionState,
    showNotice,
    withBusy
  } = options;

  const handleSelectRewriteUnit = useCallback(
    (rewriteUnitId: string, options?: { multiSelect?: boolean }) => {
      const session = currentSessionRef.current;
      setActiveRewriteUnitId(rewriteUnitId);

      if (!session) {
        setActiveSuggestionId(null);
        return;
      }

      const rewriteUnit = findRewriteUnit(session, rewriteUnitId);
      if (options?.multiSelect) {
        if (rewriteUnit && canRewriteSession(session)) {
          const targetIds = resolveSelectionTargetRewriteUnitIds(rewriteUnitId);
          setSelectedRewriteUnitIds((current) =>
            toggleSelectedRewriteUnitIds(session, current, targetIds)
          );
        }
      } else {
        setSelectedRewriteUnitIds([]);
      }

      const latestForRewriteUnit = session.suggestions
        .filter((suggestion) => suggestion.rewriteUnitId === rewriteUnitId)
        .sort((left, right) => right.sequence - left.sequence)[0];

      setActiveSuggestionId(latestForRewriteUnit?.id ?? null);
    },
    [
      currentSessionRef,
      setActiveRewriteUnitId,
      setActiveSuggestionId,
      setSelectedRewriteUnitIds
    ]
  );

  const handleSelectSuggestion = useCallback(
    (suggestionId: string) => {
      setActiveSuggestionId(suggestionId);
      setReviewView("diff");
    },
    [setActiveSuggestionId, setReviewView]
  );

  const handleApplySuggestion = useCallback(
    async (suggestionId: string) => {
      const session = currentSessionRef.current;
      if (!session) return;
      const latestSession = await refreshRewriteableSessionOrNotify({
        session,
        refreshSessionState,
        options: {
          preserveRewriteUnit: true,
          preserveSuggestion: true
        },
        showNotice,
        errorPrefix: "应用失败",
        formatError: readableError
      });
      if (!latestSession) {
        return;
      }

      const result = await runSessionActionOrNotify({
        captureDocumentScrollPosition,
        applySessionState,
        showNotice,
        errorPrefix: "应用失败",
        formatError: readableError,
        run: () =>
          withBusy(`apply-suggestion:${suggestionId}`, () =>
            applySuggestion(latestSession.id, suggestionId)
          ),
        resolveState: (updatedSession) => {
          const suggestion =
            updatedSession.suggestions.find((item) => item.id === suggestionId) ??
            getLatestSuggestion(updatedSession);
          return {
            preferredRewriteUnitId:
              suggestion?.rewriteUnitId ?? activeRewriteUnitIdRef.current,
            preferredSuggestionId: suggestionId
          };
        },
        recover: async () => {
          await refreshSessionStateSilently({
            sessionId: session.id,
            refreshSessionState,
            options: {
              preserveRewriteUnit: true,
              preserveSuggestion: true
            }
          });
        }
      });
      if (!result) {
        return;
      }

      const suggestion =
        result.session.suggestions.find((item) => item.id === suggestionId) ??
        getLatestSuggestion(result.session);
      showNotice(
        "success",
        suggestion ? `已应用修改对 #${suggestion.sequence}。` : "已应用修改对。"
      );
    },
    [
      activeRewriteUnitIdRef,
      applySessionState,
      captureDocumentScrollPosition,
      currentSessionRef,
      refreshSessionState,
      showNotice,
      withBusy
    ]
  );

  const handleDismissSuggestion = useCallback(
    async (suggestionId: string) => {
      const session = currentSessionRef.current;
      if (!session) return;

      const result = await runSessionActionOrNotify({
        captureDocumentScrollPosition,
        applySessionState,
        showNotice,
        errorPrefix: "操作失败",
        formatError: readableError,
        run: () =>
          withBusy(`dismiss-suggestion:${suggestionId}`, () =>
            dismissSuggestion(session.id, suggestionId)
          ),
        resolveState: (updatedSession) => {
          const suggestion =
            updatedSession.suggestions.find((item) => item.id === suggestionId) ??
            getLatestSuggestion(updatedSession);
          return {
            preferredRewriteUnitId:
              suggestion?.rewriteUnitId ?? activeRewriteUnitIdRef.current,
            preferredSuggestionId: suggestion?.id ?? null
          };
        }
      });
      if (!result) {
        return;
      }

      showNotice("warning", "已取消应用 / 忽略该修改对。");
    },
    [
      activeRewriteUnitIdRef,
      applySessionState,
      captureDocumentScrollPosition,
      currentSessionRef,
      showNotice,
      withBusy
    ]
  );

  const handleDeleteSuggestion = useCallback(
    async (suggestionId: string) => {
      const session = currentSessionRef.current;
      if (!session) return;

      const target = session.suggestions.find((item) => item.id === suggestionId);
      const targetRewriteUnitId = target?.rewriteUnitId ?? activeRewriteUnitIdRef.current;

      const result = await runSessionActionOrNotify({
        captureDocumentScrollPosition,
        applySessionState,
        showNotice,
        errorPrefix: "删除失败",
        formatError: readableError,
        run: () =>
          withBusy(`delete-suggestion:${suggestionId}`, () =>
            deleteSuggestion(session.id, suggestionId)
          ),
        resolveState: () => ({
          preferredRewriteUnitId: targetRewriteUnitId
        })
      });
      if (!result) {
        return;
      }

      showNotice("warning", "已删除该修改对。");
    },
    [
      activeRewriteUnitIdRef,
      applySessionState,
      captureDocumentScrollPosition,
      currentSessionRef,
      showNotice,
      withBusy
    ]
  );

  return {
    handleSelectRewriteUnit,
    handleSelectSuggestion,
    handleApplySuggestion,
    handleDismissSuggestion,
    handleDeleteSuggestion
  } as const;
}
