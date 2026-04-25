import { diffTextByLines } from "./diff";
import {
  REWRITE_UNIT_NOT_FOUND_ERROR,
  findWritebackSlot,
  type RewriteUnitResponsePayload
} from "./webBridgeProtocol";
import { normalizeTextAgainstSourceLayout } from "./webBridgeText";
import {
  applySlotUpdates,
  applySuggestionById,
  mergedTextFromSlotIds,
  randomId,
  rewriteUnitSourceText
} from "./webBridgeSessionUtils";
import type { DocumentSession, SlotUpdate, SuggestionDecision } from "./types";

function nowIso() {
  return new Date().toISOString();
}

export function normalizeRewriteUnitSlotUpdates(
  session: DocumentSession,
  updates: SlotUpdate[]
) {
  return updates.map((update) => {
    const sourceSlot = findWritebackSlot(session, update.slotId);
    return {
      slotId: update.slotId,
      text: normalizeTextAgainstSourceLayout(sourceSlot.text, update.text)
    };
  });
}

export function createRewriteSuggestion(
  session: DocumentSession,
  response: RewriteUnitResponsePayload,
  decision: SuggestionDecision
) {
  const rewriteUnit = session.rewriteUnits.find((item) => item.id === response.rewriteUnitId);
  if (!rewriteUnit) {
    throw new Error(REWRITE_UNIT_NOT_FOUND_ERROR);
  }

  const slotUpdates = normalizeRewriteUnitSlotUpdates(session, response.updates);
  const beforeText = rewriteUnitSourceText(session, rewriteUnit);
  const projectedSlots = applySlotUpdates(session.writebackSlots, slotUpdates);
  const afterText = mergedTextFromSlotIds(projectedSlots, rewriteUnit.slotIds);
  const now = nowIso();
  const suggestion = {
    id: randomId("suggestion"),
    sequence: session.nextSuggestionSequence,
    rewriteUnitId: response.rewriteUnitId,
    beforeText,
    afterText,
    diff: { spans: diffTextByLines(beforeText, afterText) },
    decision,
    slotUpdates,
    createdAt: now,
    updatedAt: now
  };
  session.nextSuggestionSequence += 1;
  return suggestion;
}

export function protectedRewriteUnitError(rewriteUnitId: string) {
  return `改写单元 ${rewriteUnitId} 属于保护区，不允许 AI 改写。`;
}

export function ensureRewriteUnitCanRewrite(session: DocumentSession, rewriteUnitId: string) {
  const unit = session.rewriteUnits.find((item) => item.id === rewriteUnitId);
  if (!unit) {
    throw new Error(REWRITE_UNIT_NOT_FOUND_ERROR);
  }
  const allSlotsLocked = unit.slotIds.every(
    (slotId) => !findWritebackSlot(session, slotId).editable
  );
  if (unit.status === "done" && allSlotsLocked) {
    throw new Error(protectedRewriteUnitError(rewriteUnitId));
  }
}

export function validateUniqueBatchSlotUpdates(results: RewriteUnitResponsePayload[]) {
  const seen = new Set<string>();
  for (const response of results) {
    for (const update of response.updates) {
      if (seen.has(update.slotId)) {
        throw new Error(
          `写回内容与原结构不一致：batch 内存在重复 slot 更新：${update.slotId}。`
        );
      }
      seen.add(update.slotId);
    }
  }
}

export function validateCandidateBatchWriteback(
  session: DocumentSession,
  responses: RewriteUnitResponsePayload[],
  deepClone: <T>(value: T) => T,
  validateSessionWriteback: (session: DocumentSession) => void
) {
  validateUniqueBatchSlotUpdates(responses);
  const preview = deepClone(session);
  for (const response of responses) {
    ensureRewriteUnitCanRewrite(preview, response.rewriteUnitId);
    const suggestion = createRewriteSuggestion(preview, response, "applied");
    preview.suggestions.push(suggestion);
    applySuggestionById(preview, suggestion.id, nowIso());
  }
  validateSessionWriteback(preview);
}
