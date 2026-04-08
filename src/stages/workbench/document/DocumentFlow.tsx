import { memo, useEffect, useMemo, useRef } from "react";
import type { ChunkTask, EditSuggestion } from "../../../lib/types";
import { summarizeChunkSuggestions } from "../../../lib/helpers";
import { isChunkSelected } from "../../../lib/chunkSelection";
import type { DocumentView } from "../hooks/useCopyDocument";
import type { ClientDocumentFormat } from "../../../lib/protectedText";
import { renderInlineProtectedText } from "../../../lib/protectedText";

interface DocumentFlowProps {
  sessionId: string;
  chunks: ChunkTask[];
  documentView: DocumentView;
  documentFormat: ClientDocumentFormat;
  showMarkers: boolean;
  suggestionsByChunk: Map<number, EditSuggestion[]>;
  runningIndexSet: Set<number>;
  optimisticManualRunningIndex: number | null;
  activeChunkIndex: number;
  selectedChunkIndices: number[];
  onSelectChunk: (index: number, options?: { multiSelect?: boolean }) => void;
  onSelectSuggestion: (suggestionId: string) => void;
}

export const DocumentFlow = memo(function DocumentFlow({
  sessionId,
  chunks,
  documentView,
  documentFormat,
  showMarkers,
  suggestionsByChunk,
  runningIndexSet,
  optimisticManualRunningIndex,
  activeChunkIndex,
  selectedChunkIndices,
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
        isChunkSelected(selectedChunkIndices, chunk.index) ? "is-selected" : "",
        chunk.skipRewrite ? "is-protected" : "",
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
    selectedChunkIndices,
    suggestionsByChunk
  ]);

  const renderText = (value: string, key: string) => {
    if (!showMarkers) return value;
    return renderInlineProtectedText(value, documentFormat, key);
  };

  return (
    <div
      className={`document-flow-wrap ${showMarkers ? "is-markers" : "is-quiet"} ${
        selectedChunkIndices.length > 0 ? "has-status" : ""
      }`}
    >
      {selectedChunkIndices.length > 0 ? (
        <div className="document-flow-status" aria-label="当前选择状态">
          <span className="context-chip" title="当前已选中的 chunk 数量">
            已选 {selectedChunkIndices.length} 段
          </span>
        </div>
      ) : null}

      {showMarkers ? (
        <div className="chunk-legend" aria-label="高亮说明">
          <span className="legend-chip is-editable" title="可改写 chunk（审阅最小单元）">
            可改写
          </span>
          <span
            className="legend-chip is-selected"
            title="按住 Ctrl / Cmd 点击可加入或移出本次处理范围"
          >
            已选中
          </span>
          <span className="legend-chip is-protected" title="保护 chunk（AI 将跳过）">
            不可改写
          </span>
          <span className="legend-chip is-inline-protected" title="行内保护区（例如 $...$）">
            行内保护
          </span>
          <span className="legend-chip is-running" title="正在生成候选稿">
            正在改写
          </span>
          {documentView === "markup" ? (
            <>
              <span className="legend-chip is-insert" title="候选稿相对原文的插入内容">
                插入
              </span>
              <span className="legend-chip is-delete" title="候选稿相对原文的删除内容">
                删除
              </span>
            </>
          ) : null}
        </div>
      ) : null}

      <p className="document-flow">
        {computed.map(({ chunk, classes, displaySuggestion }) => (
          <span key={chunk.index} className="doc-chunk-wrap">
            <span
              ref={(node) => {
                chunkNodesRef.current[chunk.index] = node;
              }}
              className={classes}
              data-chunk-index={chunk.index + 1}
              title={
                chunk.skipRewrite
                  ? "保护区：该片段将不会被 AI 修改"
                  : "可改写：点击定位；Ctrl / Cmd + 点击加入或移出本次处理范围"
              }
              onClick={(event) => {
                onSelectChunk(chunk.index, {
                  multiSelect: event.metaKey || event.ctrlKey
                });
                if (displaySuggestion) {
                  onSelectSuggestion(displaySuggestion.id);
                }
              }}
            >
              {documentView === "source"
                ? renderText(chunk.sourceText, `chunk-${chunk.index}-source`)
                : documentView === "final"
                  ? displaySuggestion
                    ? renderText(displaySuggestion.afterText, `chunk-${chunk.index}-final`)
                    : renderText(chunk.sourceText, `chunk-${chunk.index}-final-source`)
                  : displaySuggestion
                    ? displaySuggestion.diffSpans.map((span, index) => (
                        <span
                          key={`${span.type}-${index}-${span.text.length}`}
                          className={`diff-span is-${span.type}`}
                        >
                          {renderText(
                            span.text,
                            `chunk-${chunk.index}-diff-${span.type}-${index}`
                          )}
                        </span>
                      ))
                    : renderText(chunk.sourceText, `chunk-${chunk.index}-markup-source`)}
            </span>
            {chunk.separatorAfter}
          </span>
        ))}
      </p>
    </div>
  );
});
