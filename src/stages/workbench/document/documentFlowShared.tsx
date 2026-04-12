import type { ChunkTask, EditSuggestion } from "../../../lib/types";
import type { DocumentView } from "../hooks/useCopyDocument";
import type { ClientDocumentFormat } from "../../../lib/protectedText";
import { renderInlineProtectedText } from "../../../lib/protectedText";

export interface DocumentFlowBodyProps {
  chunks: ChunkTask[];
  documentView: DocumentView;
  documentFormat: ClientDocumentFormat;
  rewriteEnabled: boolean;
  rewriteBlockedReason: string | null;
  showMarkers: boolean;
  suggestionsByChunk: Map<number, EditSuggestion[]>;
  runningIndexSet: Set<number>;
  optimisticManualRunningIndex: number | null;
  activeChunkIndex: number;
  selectedChunkIndices: number[];
  onSelectChunk: (index: number, options?: { multiSelect?: boolean }) => void;
  onSelectSuggestion: (suggestionId: string) => void;
}

export function chunkTitle(
  chunk: ChunkTask,
  rewriteEnabled: boolean,
  rewriteBlockedReason: string | null
) {
  if (chunk.skipRewrite) {
    return "保护区：该片段将不会被 AI 修改";
  }
  if (rewriteEnabled) {
    return "可改写：点击定位；Ctrl / Cmd + 点击加入或移出本次处理范围";
  }
  return rewriteBlockedReason ?? "当前文档整体不可改写，仅可定位查看";
}

function renderChunkText(
  value: string,
  showMarkers: boolean,
  documentFormat: ClientDocumentFormat,
  key: string
) {
  if (!showMarkers) return value;
  return renderInlineProtectedText(value, documentFormat, key);
}

export function renderChunkContent(
  chunk: ChunkTask,
  displaySuggestion: EditSuggestion | null,
  documentView: DocumentView,
  showMarkers: boolean,
  documentFormat: ClientDocumentFormat
) {
  if (documentView === "source") {
    return renderChunkText(
      chunk.sourceText,
      showMarkers,
      documentFormat,
      `chunk-${chunk.index}-source`
    );
  }

  if (documentView === "final") {
    const value = displaySuggestion?.afterText ?? chunk.sourceText;
    const suffix = displaySuggestion ? "final" : "final-source";
    return renderChunkText(
      value,
      showMarkers,
      documentFormat,
      `chunk-${chunk.index}-${suffix}`
    );
  }

  if (!displaySuggestion) {
    return renderChunkText(
      chunk.sourceText,
      showMarkers,
      documentFormat,
      `chunk-${chunk.index}-markup-source`
    );
  }

  return displaySuggestion.diffSpans.map((span, index) => (
    <span
      key={`${chunk.index}-${span.type}-${index}-${span.text.length}`}
      className={`diff-span is-${span.type}`}
    >
      {renderChunkText(
        span.text,
        showMarkers,
        documentFormat,
        `chunk-${chunk.index}-diff-${span.type}-${index}`
      )}
    </span>
  ));
}

export function fragmentClassNames(
  chunk: ChunkTask,
  isRunning: boolean,
  isActive: boolean
) {
  return [
    "doc-paragraph-fragment",
    chunk.skipRewrite ? "is-fragment-protected" : "",
    isRunning ? "is-fragment-running" : "",
    chunk.status === "failed" ? "is-fragment-failed" : "",
    isActive ? "is-fragment-active" : ""
  ]
    .filter(Boolean)
    .join(" ");
}
