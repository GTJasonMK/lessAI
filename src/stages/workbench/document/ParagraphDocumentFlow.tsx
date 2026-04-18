import { memo, useEffect, useMemo, useRef } from "react";
import {
  logScrollRestore,
  snapshotScrollNode
} from "../../../app/hooks/documentScrollRestoreDebug";
import {
  rewriteUnitHasEditableSlot,
  summarizeRewriteUnitSuggestions
} from "../../../lib/helpers";
import { isRewriteUnitSelected } from "../../../lib/rewriteUnitSelection";
import type { SegmentationPreset } from "../../../lib/types";
import type { DocumentFlowBodyProps } from "./documentFlowShared";
import {
  shouldScrollToActiveRewriteUnit,
  type ActiveRewriteUnitTarget
} from "./documentFlowNavigation";
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

function snapshotRewriteUnitNode(node: HTMLSpanElement | null) {
  if (!node) {
    return { present: false } as const;
  }

  const rect = node.getBoundingClientRect();
  return {
    present: true,
    connected: node.isConnected,
    top: rect.top,
    bottom: rect.bottom,
    height: rect.height
  } as const;
}

function findScrollContainer(node: HTMLSpanElement | null) {
  const container = node?.closest(".paper-content");
  return container instanceof HTMLDivElement ? container : null;
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
  activeSuggestionId,
  activeReviewNavigationRequestId,
  selectedRewriteUnitIds,
  onSelectRewriteUnit,
  onSelectSuggestion
}: ParagraphDocumentFlowProps) {
  const rewriteUnitNodesRef = useRef<Record<string, HTMLSpanElement | null>>({});
  const previousActiveTargetRef = useRef<ActiveRewriteUnitTarget | null>(null);

  useEffect(() => {
    const previous = previousActiveTargetRef.current;
    const next = {
      sessionId,
      rewriteUnitId: activeRewriteUnitId,
      suggestionId: activeSuggestionId,
      navigationRequestId: activeReviewNavigationRequestId
    };
    previousActiveTargetRef.current = next;
    if (!previous) {
      logScrollRestore("paragraph-scroll-skip", {
        reason: "no-previous-target",
        next
      });
      return;
    }
    if (!shouldScrollToActiveRewriteUnit(previous, next)) {
      logScrollRestore("paragraph-scroll-skip", {
        reason: "target-unchanged",
        previous,
        next
      });
      return;
    }
    const targetRewriteUnitId = next.rewriteUnitId;
    if (!targetRewriteUnitId) {
      logScrollRestore("paragraph-scroll-skip", {
        reason: "missing-target-rewrite-unit",
        previous,
        next
      });
      return;
    }
    const targetNode = rewriteUnitNodesRef.current[targetRewriteUnitId] ?? null;
    const scrollContainer = findScrollContainer(targetNode);

    logScrollRestore("paragraph-scroll-into-view", {
      sessionId,
      previousActiveRewriteUnitId: previous?.rewriteUnitId ?? null,
      activeRewriteUnitId: targetRewriteUnitId,
      previousActiveSuggestionId: previous?.suggestionId ?? null,
      activeSuggestionId,
      previousNavigationRequestId: previous?.navigationRequestId ?? null,
      activeReviewNavigationRequestId,
      targetNode: snapshotRewriteUnitNode(targetNode),
      scrollContainer: snapshotScrollNode(scrollContainer)
    });
    if (!targetNode) {
      logScrollRestore("paragraph-scroll-skip", {
        reason: "target-node-missing",
        targetRewriteUnitId,
        knownNodeCount: Object.keys(rewriteUnitNodesRef.current).length,
        knownNodeIds: Object.keys(rewriteUnitNodesRef.current).slice(0, 12)
      });
      return;
    }
    targetNode.scrollIntoView({
      block: "center",
      behavior: "smooth"
    });
    window.requestAnimationFrame(() => {
      logScrollRestore("paragraph-scroll-after-frame", {
        sessionId,
        activeRewriteUnitId: targetRewriteUnitId,
        activeSuggestionId,
        activeReviewNavigationRequestId,
        targetNode: snapshotRewriteUnitNode(targetNode),
        scrollContainer: snapshotScrollNode(findScrollContainer(targetNode))
      });
    });
  }, [activeRewriteUnitId, activeReviewNavigationRequestId, activeSuggestionId, sessionId]);

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
