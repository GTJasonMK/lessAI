import { useCallback } from "react";
import { applySuggestion, deleteSuggestion, dismissSuggestion } from "../../lib/api";
import type { DocumentSession } from "../../lib/types";
import { canRewriteSession, getLatestSuggestion, readableError } from "../../lib/helpers";
import {
  resolveSelectionTargetChunkIndices,
  toggleSelectedChunkIndices
} from "../../lib/chunkSelection";
import type { NoticeTone } from "../../lib/constants";

type ShowNotice = (
  tone: NoticeTone,
  message: string,
  options?: { autoDismissMs?: number | null }
) => void;

type WithBusy = <T>(action: string, fn: () => Promise<T>) => Promise<T>;

type ApplySessionState = (
  session: DocumentSession,
  nextChunkIndex: number,
  options?: { preferredSuggestionId?: string | null }
) => void;

export function useSuggestionActions(options: {
  currentSessionRef: React.MutableRefObject<DocumentSession | null>;
  activeChunkIndexRef: React.MutableRefObject<number>;
  setActiveChunkIndex: React.Dispatch<React.SetStateAction<number>>;
  setActiveSuggestionId: React.Dispatch<React.SetStateAction<string | null>>;
  setSelectedChunkIndices: React.Dispatch<React.SetStateAction<number[]>>;
  setReviewView: React.Dispatch<React.SetStateAction<"diff" | "source" | "candidate">>;
  applySessionState: ApplySessionState;
  showNotice: ShowNotice;
  withBusy: WithBusy;
}) {
  const {
    currentSessionRef,
    activeChunkIndexRef,
    setActiveChunkIndex,
    setActiveSuggestionId,
    setSelectedChunkIndices,
    setReviewView,
    applySessionState,
    showNotice,
    withBusy
  } = options;

  const handleSelectChunk = useCallback(
    (index: number, options?: { multiSelect?: boolean }) => {
      const session = currentSessionRef.current;
      setActiveChunkIndex(index);

      if (!session) {
        setActiveSuggestionId(null);
        return;
      }

      const chunk = session.chunks[index];
      if (options?.multiSelect) {
        if (chunk && !chunk.skipRewrite && canRewriteSession(session)) {
          const targetIndices = resolveSelectionTargetChunkIndices(
            session.chunks,
            index,
            session.chunkPreset
          );
          setSelectedChunkIndices((current) =>
            toggleSelectedChunkIndices(current, targetIndices)
          );
        }
      } else {
        setSelectedChunkIndices([]);
      }

      let latestForChunk: { id: string; sequence: number } | null = null;
      for (const suggestion of session.suggestions) {
        if (suggestion.chunkIndex !== index) continue;
        if (!latestForChunk || suggestion.sequence > latestForChunk.sequence) {
          latestForChunk = { id: suggestion.id, sequence: suggestion.sequence };
        }
      }

      if (latestForChunk) {
        setActiveSuggestionId(latestForChunk.id);
        return;
      }

      setActiveSuggestionId(null);
    },
    [
      currentSessionRef,
      setActiveChunkIndex,
      setActiveSuggestionId,
      setSelectedChunkIndices
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

      try {
        const updated = await withBusy(`apply-suggestion:${suggestionId}`, () =>
          applySuggestion(session.id, suggestionId)
        );
        const suggestion =
          updated.suggestions.find((item) => item.id === suggestionId) ??
          getLatestSuggestion(updated);
        const chunkIndex = suggestion?.chunkIndex ?? activeChunkIndexRef.current;

        applySessionState(updated, chunkIndex, { preferredSuggestionId: suggestionId });

        showNotice(
          "success",
          suggestion ? `已应用修改对 #${suggestion.sequence}。` : "已应用修改对。"
        );
      } catch (error) {
        showNotice("error", `应用失败：${readableError(error)}`);
      }
    },
    [
      activeChunkIndexRef,
      applySessionState,
      currentSessionRef,
      showNotice,
      withBusy
    ]
  );

  const handleDismissSuggestion = useCallback(
    async (suggestionId: string) => {
      const session = currentSessionRef.current;
      if (!session) return;

      try {
        const updated = await withBusy(`dismiss-suggestion:${suggestionId}`, () =>
          dismissSuggestion(session.id, suggestionId)
        );
        const suggestion =
          updated.suggestions.find((item) => item.id === suggestionId) ??
          getLatestSuggestion(updated);
        const chunkIndex = suggestion?.chunkIndex ?? activeChunkIndexRef.current;

        applySessionState(updated, chunkIndex, {
          preferredSuggestionId: suggestion?.id ?? null
        });

        showNotice("warning", "已取消应用 / 忽略该修改对。");
      } catch (error) {
        showNotice("error", `操作失败：${readableError(error)}`);
      }
    },
    [
      activeChunkIndexRef,
      applySessionState,
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
      const targetChunkIndex = target?.chunkIndex ?? activeChunkIndexRef.current;

      try {
        const updated = await withBusy(`delete-suggestion:${suggestionId}`, () =>
          deleteSuggestion(session.id, suggestionId)
        );
        const nextChunkIndex = Math.min(
          targetChunkIndex,
          Math.max(0, updated.chunks.length - 1)
        );
        applySessionState(updated, nextChunkIndex);
        showNotice("warning", "已删除该修改对。");
      } catch (error) {
        showNotice("error", `删除失败：${readableError(error)}`);
      }
    },
    [
      activeChunkIndexRef,
      applySessionState,
      currentSessionRef,
      showNotice,
      withBusy
    ]
  );

  return {
    handleSelectChunk,
    handleSelectSuggestion,
    handleApplySuggestion,
    handleDismissSuggestion,
    handleDeleteSuggestion
  } as const;
}
