import { memo } from "react";
import type { SegmentationPreset } from "../../../lib/types";
import { countSelectedRewriteUnits } from "../../../lib/rewriteUnitSelection";
import { ParagraphDocumentFlow } from "./ParagraphDocumentFlow";
import type { DocumentFlowBodyProps } from "./documentFlowShared";

interface DocumentFlowProps extends DocumentFlowBodyProps {
  sessionId: string;
  segmentationPreset: SegmentationPreset;
}

function buildWrapClassName(showMarkers: boolean, selectedDisplayCount: number) {
  return `document-flow-wrap ${showMarkers ? "is-markers" : "is-quiet"} ${
    selectedDisplayCount > 0 ? "has-status" : ""
  }`;
}

function legendEditableLabel(rewriteEnabled: boolean) {
  return rewriteEnabled ? "可改写" : "正文片段";
}

function legendEditableTitle(rewriteEnabled: boolean, rewriteBlockedReason: string | null) {
  return rewriteEnabled
    ? "可改写 rewrite unit（审阅最小单元）"
    : rewriteBlockedReason ?? "当前文档整体不可改写";
}

function legendSelectedTitle(rewriteEnabled: boolean) {
  return rewriteEnabled
    ? "按住 Ctrl / Cmd 点击可加入或移出本次处理范围"
    : "当前文档不可改写，不支持选择处理范围";
}

export const DocumentFlow = memo(function DocumentFlow({
  sessionId,
  segmentationPreset,
  session,
  rewriteUnits,
  documentView,
  documentFormat,
  rewriteEnabled,
  rewriteBlockedReason,
  showMarkers,
  suggestionsByRewriteUnit,
  runningRewriteUnitIdSet,
  optimisticManualRunningRewriteUnitId,
  activeRewriteUnitId,
  selectedRewriteUnitIds,
  onSelectRewriteUnit,
  onSelectSuggestion
}: DocumentFlowProps) {
  const selectedDisplayCount = countSelectedRewriteUnits(selectedRewriteUnitIds);

  return (
    <div className={buildWrapClassName(showMarkers, selectedDisplayCount)}>
      {selectedDisplayCount > 0 ? (
        <div className="document-flow-status" aria-label="当前选择状态">
          <span className="context-chip" title="当前已选中的可见段落数">
            已选 {selectedDisplayCount} 段
          </span>
        </div>
      ) : null}

      {showMarkers ? (
        <div className="unit-legend" aria-label="高亮说明">
          <span
            className="legend-chip is-editable"
            title={legendEditableTitle(rewriteEnabled, rewriteBlockedReason)}
          >
            {legendEditableLabel(rewriteEnabled)}
          </span>
          <span className="legend-chip is-selected" title={legendSelectedTitle(rewriteEnabled)}>
            已选中
          </span>
          <span className="legend-chip is-protected" title="保护区（整块或行内，AI 都不会修改）">
            保护区
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
        <ParagraphDocumentFlow
          sessionId={sessionId}
          segmentationPreset={segmentationPreset}
          session={session}
          rewriteUnits={rewriteUnits}
          documentView={documentView}
          documentFormat={documentFormat}
          rewriteEnabled={rewriteEnabled}
          rewriteBlockedReason={rewriteBlockedReason}
          showMarkers={showMarkers}
          suggestionsByRewriteUnit={suggestionsByRewriteUnit}
          runningRewriteUnitIdSet={runningRewriteUnitIdSet}
          optimisticManualRunningRewriteUnitId={optimisticManualRunningRewriteUnitId}
          activeRewriteUnitId={activeRewriteUnitId}
          selectedRewriteUnitIds={selectedRewriteUnitIds}
          onSelectRewriteUnit={onSelectRewriteUnit}
          onSelectSuggestion={onSelectSuggestion}
        />
      </p>
    </div>
  );
});
