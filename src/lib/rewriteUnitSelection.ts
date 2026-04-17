import type { DocumentSession, RewriteUnit } from "./types";
import { rewriteUnitHasEditableSlot } from "./helpers";

function buildSelectedSet(selectedRewriteUnitIds: readonly string[]) {
  return new Set(selectedRewriteUnitIds);
}

function sortRewriteUnitIds(units: readonly RewriteUnit[], unitIds: Set<string>) {
  const order = new Map(units.map((unit, index) => [unit.id, index] as const));
  return Array.from(unitIds).sort((left, right) => (order.get(left) ?? 0) - (order.get(right) ?? 0));
}

function isRewriteUnitSelectable(session: DocumentSession, rewriteUnit: RewriteUnit) {
  return rewriteUnitHasEditableSlot(session, rewriteUnit);
}

export function toggleSelectedRewriteUnitIds(
  session: DocumentSession,
  selectedRewriteUnitIds: readonly string[],
  targetRewriteUnitIds: readonly string[]
) {
  const selected = buildSelectedSet(selectedRewriteUnitIds);
  const selectableTargetIds = targetRewriteUnitIds.filter((rewriteUnitId) => {
    const rewriteUnit = session.rewriteUnits.find((item) => item.id === rewriteUnitId);
    return rewriteUnit ? isRewriteUnitSelectable(session, rewriteUnit) : false;
  });
  const allSelected =
    selectableTargetIds.length > 0 &&
    selectableTargetIds.every((rewriteUnitId) => selected.has(rewriteUnitId));

  for (const rewriteUnitId of selectableTargetIds) {
    if (allSelected) {
      selected.delete(rewriteUnitId);
      continue;
    }
    selected.add(rewriteUnitId);
  }

  return sortRewriteUnitIds(session.rewriteUnits, selected);
}

export function normalizeSelectedRewriteUnitIds(
  session: DocumentSession,
  selectedRewriteUnitIds: readonly string[]
) {
  const allowed = new Set(
    session.rewriteUnits
      .filter((rewriteUnit) => isRewriteUnitSelectable(session, rewriteUnit))
      .map((rewriteUnit) => rewriteUnit.id)
  );

  const normalized = new Set<string>();
  for (const rewriteUnitId of selectedRewriteUnitIds) {
    if (!allowed.has(rewriteUnitId)) continue;
    normalized.add(rewriteUnitId);
  }

  return sortRewriteUnitIds(session.rewriteUnits, normalized);
}

export function hasSelectedRewriteUnits(selectedRewriteUnitIds: readonly string[]) {
  return selectedRewriteUnitIds.length > 0;
}

export function isRewriteUnitSelected(
  selectedRewriteUnitIds: readonly string[],
  rewriteUnitId: string
) {
  return selectedRewriteUnitIds.includes(rewriteUnitId);
}

export function resolveSelectionTargetRewriteUnitIds(rewriteUnitId: string) {
  return [rewriteUnitId];
}

function matchesTarget(
  session: DocumentSession,
  selectedRewriteUnitIds: readonly string[],
  rewriteUnit: RewriteUnit
) {
  return (
    isRewriteUnitSelectable(session, rewriteUnit) &&
    (!hasSelectedRewriteUnits(selectedRewriteUnitIds) ||
      selectedRewriteUnitIds.includes(rewriteUnit.id))
  );
}

export function countSelectedRewriteUnits(selectedRewriteUnitIds: readonly string[]) {
  return selectedRewriteUnitIds.length;
}

export function findNextManualTargetRewriteUnit(
  session: DocumentSession,
  selectedRewriteUnitIds: readonly string[]
) {
  return (
    session.rewriteUnits.find(
      (rewriteUnit) =>
        matchesTarget(session, selectedRewriteUnitIds, rewriteUnit) &&
        (rewriteUnit.status === "idle" || rewriteUnit.status === "failed")
    ) ?? null
  );
}

export function resolveOptimisticManualRunningRewriteUnitId(
  session: DocumentSession,
  selectedRewriteUnitIds: readonly string[]
) {
  return findNextManualTargetRewriteUnit(session, selectedRewriteUnitIds)?.id ?? null;
}

export function findAutoPendingTargetRewriteUnits(
  session: DocumentSession,
  selectedRewriteUnitIds: readonly string[]
) {
  return session.rewriteUnits.filter(
    (rewriteUnit) =>
      matchesTarget(session, selectedRewriteUnitIds, rewriteUnit) &&
      rewriteUnit.status !== "done"
  );
}
