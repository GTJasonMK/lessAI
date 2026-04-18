export interface ActiveRewriteUnitTarget {
  sessionId: string;
  rewriteUnitId: string | null;
  suggestionId: string | null;
  navigationRequestId: number;
}

export function shouldScrollToActiveRewriteUnit(
  previous: ActiveRewriteUnitTarget | null,
  next: ActiveRewriteUnitTarget
) {
  if (!previous) return false;
  if (previous.sessionId !== next.sessionId) return false;
  if (!next.rewriteUnitId) return false;

  return (
    previous.rewriteUnitId !== next.rewriteUnitId ||
    previous.suggestionId !== next.suggestionId ||
    previous.navigationRequestId !== next.navigationRequestId
  );
}
