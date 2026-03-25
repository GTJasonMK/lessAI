import { memo, useEffect, useMemo, useRef } from "react";
import type { ChunkTask, EditSuggestion } from "../../../lib/types";
import { summarizeChunkSuggestions } from "../../../lib/helpers";
import type { DocumentView } from "../hooks/useCopyDocument";

interface DocumentFlowProps {
  sessionId: string;
  chunks: ChunkTask[];
  documentView: DocumentView;
  suggestionsByChunk: Map<number, EditSuggestion[]>;
  runningIndexSet: Set<number>;
  optimisticManualRunningIndex: number | null;
  activeChunkIndex: number;
  onSelectChunk: (index: number) => void;
  onSelectSuggestion: (suggestionId: string) => void;
}

export const DocumentFlow = memo(function DocumentFlow({
  sessionId,
  chunks,
  documentView,
  suggestionsByChunk,
  runningIndexSet,
  optimisticManualRunningIndex,
  activeChunkIndex,
  onSelectChunk,
  onSelectSuggestion
}: DocumentFlowProps) {
  const chunkNodesRef = useRef<Array<HTMLSpanElement | null>>([]);

  useEffect(() => {
    const node = chunkNodesRef.current[activeChunkIndex];
    node?.scrollIntoView({ block: "center", behavior: "smooth" });
  }, [activeChunkIndex, sessionId]);

  const computed = useMemo(() => {
    return chunks.map((chunk) => {
      const chunkSuggestions = suggestionsByChunk.get(chunk.index) ?? [];
      const summary = summarizeChunkSuggestions(chunkSuggestions);
      const displaySuggestion = summary.applied ?? summary.proposed ?? null;
      const isRunning =
        chunk.status === "running" ||
        runningIndexSet.has(chunk.index) ||
        chunk.index === optimisticManualRunningIndex;

      const classes = [
        "doc-chunk",
        chunk.index === activeChunkIndex ? "is-active" : "",
        isRunning ? "is-running" : "",
        chunk.status === "failed" ? "is-failed" : "",
        documentView === "markup" && summary.applied ? "is-applied" : "",
        documentView === "markup" && !summary.applied && summary.proposed ? "is-proposed" : ""
      ]
        .filter(Boolean)
        .join(" ");

      return { chunk, classes, displaySuggestion };
    });
  }, [
    activeChunkIndex,
    chunks,
    documentView,
    optimisticManualRunningIndex,
    runningIndexSet,
    suggestionsByChunk
  ]);

  return (
    <p className="document-flow">
      {computed.map(({ chunk, classes, displaySuggestion }) => (
        <span key={chunk.index} className="doc-chunk-wrap">
          <span
            ref={(node) => {
              chunkNodesRef.current[chunk.index] = node;
            }}
            className={classes}
            onClick={() => {
              onSelectChunk(chunk.index);
              if (displaySuggestion) {
                onSelectSuggestion(displaySuggestion.id);
              }
            }}
          >
            {documentView === "source"
              ? chunk.sourceText
              : documentView === "final"
                ? displaySuggestion
                  ? displaySuggestion.afterText
                  : chunk.sourceText
                : displaySuggestion
                  ? displaySuggestion.diffSpans.map((span, index) => (
                      <span
                        key={`${span.type}-${index}-${span.text.length}`}
                        className={`diff-span is-${span.type}`}
                      >
                        {span.text}
                      </span>
                    ))
                  : chunk.sourceText}
          </span>
          {chunk.separatorAfter}
        </span>
      ))}
    </p>
  );
});

