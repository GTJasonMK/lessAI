import type { ReactNode } from "react";
import type { DocumentSession, RewriteSuggestion, RewriteUnit, WritebackSlot } from "../../../lib/types";
import type { DocumentView } from "../hooks/useCopyDocument";
import type { ClientDocumentFormat } from "../../../lib/protectedText";
import { renderInlineProtectedText } from "../../../lib/protectedText";
import {
  rewriteUnitSlotsWithSuggestion,
  rewriteUnitHasEditableSlot
} from "../../../lib/helpers";

export interface DocumentFlowBodyProps {
  session: DocumentSession;
  rewriteUnits: RewriteUnit[];
  documentView: DocumentView;
  documentFormat: ClientDocumentFormat;
  rewriteEnabled: boolean;
  rewriteBlockedReason: string | null;
  showMarkers: boolean;
  suggestionsByRewriteUnit: Map<string, RewriteSuggestion[]>;
  runningRewriteUnitIdSet: Set<string>;
  optimisticManualRunningRewriteUnitId: string | null;
  activeRewriteUnitId: string | null;
  selectedRewriteUnitIds: string[];
  onSelectRewriteUnit: (rewriteUnitId: string, options?: { multiSelect?: boolean }) => void;
  onSelectSuggestion: (suggestionId: string) => void;
}

function slotPresentationClass(slot: WritebackSlot) {
  const presentation = slot.presentation;
  return [
    "doc-paragraph-fragment",
    slot.editable ? "" : "is-fragment-protected",
    presentation?.bold ? "is-bold" : "",
    presentation?.italic ? "is-italic" : "",
    presentation?.underline ? "is-underline" : "",
    presentation?.href ? "is-link" : ""
  ]
    .filter(Boolean)
    .join(" ");
}

export function rewriteUnitTitle(
  session: DocumentSession,
  rewriteUnit: RewriteUnit,
  rewriteEnabled: boolean,
  rewriteBlockedReason: string | null
) {
  if (!rewriteUnitHasEditableSlot(session, rewriteUnit)) {
    return "保护区：该片段将不会被 AI 修改";
  }
  if (rewriteEnabled) {
    return "可改写：点击定位；Ctrl / Cmd + 点击加入或移出本次处理范围";
  }
  return rewriteBlockedReason ?? "当前文档整体不可改写，仅可定位查看";
}

function renderSlotText(
  value: string,
  showMarkers: boolean,
  documentFormat: ClientDocumentFormat,
  key: string
) {
  if (!showMarkers) return value;
  return renderInlineProtectedText(value, documentFormat, key);
}

function renderSlots(
  slots: ReadonlyArray<WritebackSlot>,
  showMarkers: boolean,
  documentFormat: ClientDocumentFormat,
  keyPrefix: string
) {
  return slots.map((slot) => (
    <span key={`${keyPrefix}-${slot.id}`} className={slotPresentationClass(slot)}>
      {renderSlotText(slot.text, showMarkers, documentFormat, `${keyPrefix}-${slot.id}`)}
    </span>
  ));
}

function rewriteUnitSeparatorText(slots: ReadonlyArray<WritebackSlot>) {
  return slots.map((slot) => slot.separatorAfter).join("");
}

function trimTrailingSeparatorFromDiffSpans(
  diffSpans: RewriteSuggestion["diffSpans"],
  separatorText: string
) {
  if (!separatorText) return diffSpans;

  let remaining = separatorText.length;
  const trimmed = diffSpans.map((span) => ({ ...span }));
  for (let index = trimmed.length - 1; index >= 0 && remaining > 0; index -= 1) {
    const text = trimmed[index].text;
    if (!text) continue;
    const trimCount = Math.min(remaining, text.length);
    trimmed[index].text = text.slice(0, text.length - trimCount);
    remaining -= trimCount;
  }

  return trimmed.filter((span) => span.text.length > 0);
}

export interface RenderedRewriteUnitContent {
  body: ReactNode;
  separatorText: string;
}

export function renderRewriteUnitContent(
  session: DocumentSession,
  rewriteUnit: RewriteUnit,
  displaySuggestion: RewriteSuggestion | null,
  documentView: DocumentView,
  showMarkers: boolean,
  documentFormat: ClientDocumentFormat
) : RenderedRewriteUnitContent {
  const slots = rewriteUnitSlotsWithSuggestion(
    session,
    rewriteUnit,
    documentView === "final" ? displaySuggestion : null
  );
  const separatorText = rewriteUnitSeparatorText(slots);

  if (documentView === "markup" && displaySuggestion) {
    return {
      body: trimTrailingSeparatorFromDiffSpans(displaySuggestion.diffSpans, separatorText).map(
        (span, index) => (
          <span
            key={`${rewriteUnit.id}-${span.type}-${index}-${span.text.length}`}
            className={`diff-span is-${span.type}`}
          >
            {renderSlotText(
              span.text,
              showMarkers,
              documentFormat,
              `${rewriteUnit.id}-diff-${span.type}-${index}`
            )}
          </span>
        )
      ),
      separatorText
    };
  }

  return {
    body: renderSlots(
      slots,
      showMarkers,
      documentFormat,
      `${rewriteUnit.id}-${documentView}`
    ),
    separatorText
  };
}
