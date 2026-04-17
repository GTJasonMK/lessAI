import { memo, useEffect, useMemo, useRef } from "react";
import { logScrollRestore } from "../../../app/hooks/documentScrollRestoreDebug";
import {
  rewriteUnitHasEditableSlot,
  summarizeRewriteUnitSuggestions
} from "../../../lib/helpers";
import { isRewriteUnitSelected } from "../../../lib/rewriteUnitSelection";
import type { SegmentationPreset } from "../../../lib/types";
import type { DocumentFlowBodyProps } from "./documentFlowShared";
import {
  renderRewriteUnitContent,
  rewriteUnitTitle
} from "./documentFlowShared";

interface ParagraphDocumentFlowProps extends DocumentFlowBodyProps {
  sessionId: string;
  segmentationPreset: SegmentationPreset;
}

function buildRewriteUnitClassNames(
  hasActiveRewriteUnit: boolean,
  hasSelectedRewriteUnit: boolean,
  hasEditableSlot: boolean,
  isRunning: boolean,
  isFailed: boolean,
  hasAppliedSuggestion: boolean,
  hasProposedSuggestion: boolean,
  documentView: DocumentFlowBodyProps["documentView"]
) {
  return [
    "doc-unit",
    "doc-paragraph-unit",
    hasActiveRewriteUnit ? "is-active" : "",
    hasSelectedRewriteUnit ? "is-selected" : "",
    !hasEditableSlot ? "is-protected" : "",
    isRunning ? "is-running" : "",
    isFailed ? "is-failed" : "",
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
}: ParagraphDocumentFlowProps) {
  const rewriteUnitNodesRef = useRef<Record<string, HTMLSpanElement | null>>({});
  const previousActiveTargetRef = useRef<{ sessionId: string; activeRewriteUnitId: string | null } | null>(
    null
  );

  useEffect(() => {
    const previous = previousActiveTargetRef.current;
    previousActiveTargetRef.current = { sessionId, activeRewriteUnitId };
    if (!previous) return;
    if (previous.sessionId !== sessionId) return;
    if (previous.activeRewriteUnitId === activeRewriteUnitId) return;
    if (!activeRewriteUnitId) return;

    logScrollRestore("paragraph-scroll-into-view", {
      sessionId,
      previousActiveRewriteUnitId: previous.activeRewriteUnitId,
      activeRewriteUnitId
    });
    rewriteUnitNodesRef.current[activeRewriteUnitId]?.scrollIntoView({
      block: "center",
      behavior: "smooth"
    });
  }, [activeRewriteUnitId, sessionId]);

  const computed = useMemo(
    () =>
      rewriteUnits.map((rewriteUnit) => {
        const unitSuggestions = suggestionsByRewriteUnit.get(rewriteUnit.id) ?? [];
        const summary = summarizeRewriteUnitSuggestions(unitSuggestions);
        const displaySuggestion = summary.applied ?? summary.proposed ?? null;
        const isRunning =
          rewriteUnit.status === "running" ||
          runningRewriteUnitIdSet.has(rewriteUnit.id) ||
          rewriteUnit.id === optimisticManualRunningRewriteUnitId;
        const hasEditableSlot = rewriteUnitHasEditableSlot(session, rewriteUnit);

        return {
          rewriteUnit,
          displaySuggestion,
          isRunning,
          classes: buildRewriteUnitClassNames(
            rewriteUnit.id === activeRewriteUnitId,
            isRewriteUnitSelected(selectedRewriteUnitIds, rewriteUnit.id),
            hasEditableSlot,
            isRunning,
            rewriteUnit.status === "failed",
            Boolean(summary.applied),
            Boolean(summary.proposed),
            documentView
          )
        };
      }),
    [
      activeRewriteUnitId,
      documentView,
      optimisticManualRunningRewriteUnitId,
      rewriteUnits,
      runningRewriteUnitIdSet,
      selectedRewriteUnitIds,
      session,
      suggestionsByRewriteUnit
    ]
  );

  return computed.map(({ rewriteUnit, displaySuggestion, classes }) => {
    const rendered = renderRewriteUnitContent(
      session,
      rewriteUnit,
      displaySuggestion,
      documentView,
      showMarkers,
      documentFormat
    );

    return (
      <span key={rewriteUnit.id} className="doc-unit-wrap">
        <span
          ref={(node) => {
            rewriteUnitNodesRef.current[rewriteUnit.id] = node;
          }}
          className={classes}
          data-rewrite-unit-id={rewriteUnit.id}
          title={rewriteUnitTitle(session, rewriteUnit, rewriteEnabled, rewriteBlockedReason)}
          onClick={(event) => {
            onSelectRewriteUnit(rewriteUnit.id, {
              multiSelect: event.metaKey || event.ctrlKey
            });
            if (displaySuggestion) {
              onSelectSuggestion(displaySuggestion.id);
            }
          }}
        >
          {rendered.body}
        </span>
        {rendered.separatorText ? (
          <span className="doc-unit-separator">{rendered.separatorText}</span>
        ) : null}
      </span>
    );
  });
});
