import type { NoticeTone } from "../../lib/constants";
import type { DocumentSession } from "../../lib/types";
import {
  canRewriteSession,
  rewriteBlockedReason,
  selectDefaultRewriteUnitId
} from "../../lib/helpers";

export type ShowNotice = (
  tone: NoticeTone,
  message: string,
  options?: { autoDismissMs?: number | null }
) => void;

export type WithBusy = <T>(action: string, fn: () => Promise<T>) => Promise<T>;

export type ApplySessionState = (
  session: DocumentSession,
  nextRewriteUnitId: string | null,
  options?: {
    preferredSuggestionId?: string | null;
    preservedScrollTop?: number | null;
  }
) => void;

export interface RefreshSessionOptions {
  preserveRewriteUnit?: boolean;
  preferredRewriteUnitId?: string | null;
  preserveSuggestion?: boolean;
  preferredSuggestionId?: string | null;
  preserveScroll?: boolean;
}

export type RefreshSessionState = (
  sessionId: string,
  options?: RefreshSessionOptions
) => Promise<DocumentSession>;

interface ApplyUpdatedSessionStateOptions {
  session: DocumentSession;
  applySessionState: ApplySessionState;
  preferredRewriteUnitId?: string | null;
  preferredSuggestionId?: string | null;
  preservedScrollTop?: number | null;
}

export function resolveNextRewriteUnitId(
  session: DocumentSession,
  preferredRewriteUnitId?: string | null
) {
  return preferredRewriteUnitId &&
    session.rewriteUnits.some((rewriteUnit) => rewriteUnit.id === preferredRewriteUnitId)
    ? preferredRewriteUnitId
    : selectDefaultRewriteUnitId(session);
}

export function applyUpdatedSessionState({
  session,
  applySessionState,
  preferredRewriteUnitId,
  preferredSuggestionId,
  preservedScrollTop
}: ApplyUpdatedSessionStateOptions) {
  applySessionState(session, resolveNextRewriteUnitId(session, preferredRewriteUnitId), {
    preferredSuggestionId,
    preservedScrollTop
  });
}

interface RunSessionActionWithScrollOptions {
  captureDocumentScrollPosition: () => number | null;
  run: () => Promise<DocumentSession>;
  applySessionState: ApplySessionState;
  preservedScrollTop?: number | null;
  resolveState?: (session: DocumentSession) => {
    preferredRewriteUnitId?: string | null;
    preferredSuggestionId?: string | null;
  };
}

export async function runSessionActionWithScroll({
  captureDocumentScrollPosition,
  run,
  applySessionState,
  preservedScrollTop,
  resolveState
}: RunSessionActionWithScrollOptions) {
  const nextPreservedScrollTop =
    preservedScrollTop === undefined
      ? captureDocumentScrollPosition()
      : preservedScrollTop;
  const session = await run();
  const nextState = resolveState?.(session);
  applyUpdatedSessionState({
    session,
    applySessionState,
    preferredRewriteUnitId: nextState?.preferredRewriteUnitId,
    preferredSuggestionId: nextState?.preferredSuggestionId,
    preservedScrollTop: nextPreservedScrollTop
  });
  return { session, preservedScrollTop: nextPreservedScrollTop };
}

interface RestoreLoadedSessionWithScrollOptions {
  captureDocumentScrollPosition: () => number | null;
  applySessionState: ApplySessionState;
  session: DocumentSession;
  preservedScrollTop?: number | null;
  preferredRewriteUnitId?: string | null;
  preferredSuggestionId?: string | null;
}

export async function restoreLoadedSessionWithScroll({
  captureDocumentScrollPosition,
  applySessionState,
  session,
  preservedScrollTop,
  preferredRewriteUnitId,
  preferredSuggestionId
}: RestoreLoadedSessionWithScrollOptions) {
  return runSessionActionWithScroll({
    captureDocumentScrollPosition,
    applySessionState,
    preservedScrollTop,
    run: async () => session,
    resolveState: () => ({
      preferredRewriteUnitId,
      preferredSuggestionId
    })
  });
}

interface RefreshSessionOrNotifyOptions {
  session: DocumentSession;
  refreshSessionState: RefreshSessionState;
  options?: RefreshSessionOptions;
  showNotice: ShowNotice;
  errorPrefix: string;
  formatError: (error: unknown) => string;
}

export async function refreshSessionOrNotify({
  session,
  refreshSessionState,
  options,
  showNotice,
  errorPrefix,
  formatError
}: RefreshSessionOrNotifyOptions): Promise<DocumentSession | null> {
  try {
    return await refreshSessionState(session.id, options);
  } catch (error) {
    showNotice("error", `${errorPrefix}：${formatError(error)}`);
    return null;
  }
}

interface RefreshSessionStateSilentlyOptions {
  sessionId: string;
  refreshSessionState: RefreshSessionState;
  options?: RefreshSessionOptions;
  afterRefresh?: (session: DocumentSession) => void | Promise<void>;
}

export async function refreshSessionStateSilently({
  sessionId,
  refreshSessionState,
  options,
  afterRefresh
}: RefreshSessionStateSilentlyOptions): Promise<DocumentSession | null> {
  try {
    const session = await refreshSessionState(sessionId, options);
    await afterRefresh?.(session);
    return session;
  } catch {
    return null;
  }
}

interface RefreshAllowedSessionOrNotifyOptions extends RefreshSessionOrNotifyOptions {
  allowed: (session: DocumentSession) => boolean;
  blockedMessage: (session: DocumentSession) => string | null | undefined;
  fallbackMessage: string;
}

export async function refreshAllowedSessionOrNotify({
  session,
  refreshSessionState,
  options,
  showNotice,
  errorPrefix,
  formatError,
  allowed,
  blockedMessage,
  fallbackMessage
}: RefreshAllowedSessionOrNotifyOptions): Promise<DocumentSession | null> {
  const latestSession = await refreshSessionOrNotify({
    session,
    refreshSessionState,
    options,
    showNotice,
    errorPrefix,
    formatError
  });
  if (!latestSession) {
    return null;
  }
  if (
    !ensureAllowedOrNotify({
      allowed: allowed(latestSession),
      blockedMessage: blockedMessage(latestSession),
      fallbackMessage,
      showNotice
    })
  ) {
    return null;
  }
  return latestSession;
}

interface RefreshRewriteableSessionOrNotifyOptions extends RefreshSessionOrNotifyOptions {}

export async function refreshRewriteableSessionOrNotify({
  session,
  refreshSessionState,
  options,
  showNotice,
  errorPrefix,
  formatError
}: RefreshRewriteableSessionOrNotifyOptions): Promise<DocumentSession | null> {
  return refreshAllowedSessionOrNotify({
    session,
    refreshSessionState,
    options,
    showNotice,
    errorPrefix,
    formatError,
    allowed: canRewriteSession,
    blockedMessage: rewriteBlockedReason,
    fallbackMessage: "当前文档暂不支持安全写回覆盖，因此不允许继续 AI 改写。"
  });
}

interface RunSessionActionOrNotifyOptions extends RunSessionActionWithScrollOptions {
  showNotice: ShowNotice;
  errorPrefix: string;
  formatError: (error: unknown) => string;
  recover?: (error: unknown) => void | Promise<void>;
}

export async function runSessionActionOrNotify({
  showNotice,
  errorPrefix,
  formatError,
  recover,
  ...options
}: RunSessionActionOrNotifyOptions): Promise<{
  session: DocumentSession;
  preservedScrollTop: number | null;
} | null> {
  try {
    return await runSessionActionWithScroll(options);
  } catch (error) {
    await recover?.(error);
    showNotice("error", `${errorPrefix}：${formatError(error)}`);
    return null;
  }
}

interface EnsureAllowedOrNotifyOptions {
  allowed: boolean;
  blockedMessage: string | null | undefined;
  fallbackMessage: string;
  showNotice: ShowNotice;
}

export function ensureAllowedOrNotify({
  allowed,
  blockedMessage,
  fallbackMessage,
  showNotice
}: EnsureAllowedOrNotifyOptions): boolean {
  if (allowed) return true;
  showNotice("warning", blockedMessage ?? fallbackMessage);
  return false;
}
