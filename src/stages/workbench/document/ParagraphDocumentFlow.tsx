import { memo, useEffect, useMemo, useRef } from "react";
import { summarizeChunkSuggestions } from "../../../lib/helpers";
import { buildChunkGroups } from "../../../lib/chunkGroups";
import { isChunkSelected } from "../../../lib/chunkSelection";
import type { ChunkPreset } from "../../../lib/types";
import type { DocumentFlowBodyProps } from "./documentFlowShared";
import {
  chunkTitle,
  fragmentClassNames,
  renderChunkContent
} from "./documentFlowShared";

interface ParagraphDocumentFlowProps extends DocumentFlowBodyProps {
  sessionId: string;
  chunkPreset: ChunkPreset;
}

function buildGroupClassNames(
  hasActiveChunk: boolean,
  hasSelectedChunk: boolean,
  hasEditableChunk: boolean,
  hasRunningChunk: boolean,
  hasFailedChunk: boolean,
  hasAppliedSuggestion: boolean,
  hasProposedSuggestion: boolean,
  documentView: DocumentFlowBodyProps["documentView"]
) {
  return [
    "doc-chunk",
    "doc-paragraph-chunk",
    hasActiveChunk ? "is-active" : "",
    hasSelectedChunk ? "is-selected" : "",
    !hasEditableChunk ? "is-protected" : "",
    hasRunningChunk ? "is-running" : "",
    hasFailedChunk ? "is-failed" : "",
    documentView === "markup" && hasAppliedSuggestion ? "is-applied" : "",
    documentView === "markup" && !hasAppliedSuggestion && hasProposedSuggestion
      ? "is-proposed"
      : ""
  ]
    .filter(Boolean)
    .join(" ");
}

export const ParagraphDocumentFlow = memo(function ParagraphDocumentFlow({
  sessionId,
  chunkPreset,
  chunks,
  documentView,
  documentFormat,
  rewriteEnabled,
  rewriteBlockedReason,
  showMarkers,
  suggestionsByChunk,
  runningIndexSet,
  optimisticManualRunningIndex,
  activeChunkIndex,
  selectedChunkIndices,
  onSelectChunk,
  onSelectSuggestion
}: ParagraphDocumentFlowProps) {
  const groupNodesRef = useRef<Record<string, HTMLSpanElement | null>>({});

  const groups = useMemo(() => buildChunkGroups(chunks, chunkPreset), [chunkPreset, chunks]);

  useEffect(() => {
    const activeGroup = groups.find((group) => group.chunkIndices.includes(activeChunkIndex));
    if (!activeGroup) return;
    groupNodesRef.current[activeGroup.id]?.scrollIntoView({
      block: "center",
      behavior: "smooth"
    });
  }, [activeChunkIndex, groups, sessionId]);

  const computed = useMemo(
    () =>
      groups.map((group) => {
        const fragments = group.chunks.map((chunk) => {
          const chunkSuggestions = suggestionsByChunk.get(chunk.index) ?? [];
          const summary = summarizeChunkSuggestions(chunkSuggestions);
          const displaySuggestion = summary.applied ?? summary.proposed ?? null;
          const isRunning =
            chunk.status === "running" ||
            runningIndexSet.has(chunk.index) ||
            chunk.index === optimisticManualRunningIndex;

          return {
            chunk,
            displaySuggestion,
            isRunning,
            hasApplied: Boolean(summary.applied),
            hasProposed: Boolean(summary.proposed)
          };
        });

        return {
          group,
          fragments,
          classes: buildGroupClassNames(
            group.chunkIndices.includes(activeChunkIndex),
            group.editableIndices.some((index) => isChunkSelected(selectedChunkIndices, index)),
            group.editableIndices.length > 0,
            fragments.some((fragment) => fragment.isRunning),
            group.chunks.some((chunk) => chunk.status === "failed"),
            fragments.some((fragment) => fragment.hasApplied),
            fragments.some((fragment) => fragment.hasProposed),
            documentView
          ),
          trailingSeparator: group.chunks[group.chunks.length - 1]?.separatorAfter ?? ""
        };
      }),
    [
      activeChunkIndex,
      documentView,
      groups,
      optimisticManualRunningIndex,
      runningIndexSet,
      selectedChunkIndices,
      suggestionsByChunk
    ]
  );

  return computed.map(({ group, fragments, classes, trailingSeparator }) => (
    <span key={group.id} className="doc-chunk-wrap">
      <span
        ref={(node) => {
          groupNodesRef.current[group.id] = node;
        }}
        className={classes}
      >
        {fragments.map((fragment, index) => {
          const suffix = index + 1 < fragments.length ? fragment.chunk.separatorAfter : "";
          return (
            <span
              key={fragment.chunk.index}
              className={fragmentClassNames(
                fragment.chunk,
                fragment.isRunning,
                fragment.chunk.index === activeChunkIndex
              )}
              data-chunk-index={fragment.chunk.index + 1}
              title={chunkTitle(fragment.chunk, rewriteEnabled, rewriteBlockedReason)}
              onClick={(event) => {
                onSelectChunk(fragment.chunk.index, {
                  multiSelect: event.metaKey || event.ctrlKey
                });
                if (fragment.displaySuggestion) {
                  onSelectSuggestion(fragment.displaySuggestion.id);
                }
              }}
            >
              {renderChunkContent(
                fragment.chunk,
                fragment.displaySuggestion,
                documentView,
                showMarkers,
                documentFormat
              )}
              {suffix}
            </span>
          );
        })}
      </span>
      {trailingSeparator}
    </span>
  ));
});
