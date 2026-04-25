import type { AppSettings, DocumentSession } from "./types";

interface BuildCleanSessionInput {
  id: string;
  path: string;
  title: string;
  sourceText: string;
  settings: AppSettings;
  createdAt?: string;
}

interface SessionCommandDeps {
  sessions: Map<string, DocumentSession>;
  deepClone: <T>(value: T) => T;
  nowIso: () => string;
  getSettings: () => AppSettings;
  extractFileTitle: (path: string) => string;
  sessionIdFromPath: (path: string) => string;
  buildCleanSession: (params: BuildCleanSessionInput) => DocumentSession;
  getVirtualFile: (path: string) => { title: string; text: string } | null;
  loadSessionInternal: (sessionId: string) => Promise<DocumentSession>;
  ensureNoActiveJob: (sessionId: string, errorMessage: string) => void;
  activeJobResetSessionError: string;
  getSessionOrThrow: (sessionId: string) => DocumentSession;
  applySuggestionById: (
    session: DocumentSession,
    suggestionId: string,
    now: string
  ) => string;
  validateSessionWriteback: (session: DocumentSession) => void;
  updateSessionTimestamp: (session: DocumentSession) => void;
  findSuggestionIndex: (session: DocumentSession, suggestionId: string) => number;
}

export function createSessionCommands(deps: SessionCommandDeps) {
  async function openDocumentCommand(path: string) {
    if (!path.trim()) {
      throw new Error("文件路径不能为空。");
    }
    const file = deps.getVirtualFile(path);
    if (!file) {
      throw new Error("网页缓存中未找到该 TXT 文件，请重新选择文件。");
    }

    const sessionId = deps.sessionIdFromPath(path);
    const existing = deps.sessions.get(sessionId);
    if (existing) {
      const refreshed = await deps.loadSessionInternal(sessionId);
      return deps.deepClone(refreshed);
    }

    const session = deps.buildCleanSession({
      id: sessionId,
      path,
      title: file.title || deps.extractFileTitle(path),
      sourceText: file.text,
      settings: deps.getSettings()
    });
    deps.sessions.set(sessionId, session);
    return deps.deepClone(session);
  }

  async function loadSessionCommand(sessionId: string) {
    const session = await deps.loadSessionInternal(sessionId);
    return deps.deepClone(session);
  }

  async function resetSessionCommand(sessionId: string) {
    deps.ensureNoActiveJob(sessionId, deps.activeJobResetSessionError);
    const existing = deps.getSessionOrThrow(sessionId);
    const file = deps.getVirtualFile(existing.documentPath);
    if (!file) {
      throw new Error("网页缓存中未找到该 TXT 文件。");
    }
    const session = deps.buildCleanSession({
      id: existing.id,
      path: existing.documentPath,
      title: existing.title,
      sourceText: file.text,
      settings: deps.getSettings()
    });
    deps.sessions.set(sessionId, session);
    return deps.deepClone(session);
  }

  async function applySuggestionCommand(sessionId: string, suggestionId: string) {
    const session = await deps.loadSessionInternal(sessionId);
    const rollback = deps.deepClone(session);
    const now = deps.nowIso();
    try {
      deps.applySuggestionById(session, suggestionId, now);
      deps.validateSessionWriteback(session);
      deps.updateSessionTimestamp(session);
      return deps.deepClone(session);
    } catch (error) {
      deps.sessions.set(sessionId, rollback);
      throw error;
    }
  }

  async function dismissSuggestionCommand(sessionId: string, suggestionId: string) {
    const session = deps.getSessionOrThrow(sessionId);
    const index = deps.findSuggestionIndex(session, suggestionId);
    if (index < 0) {
      throw new Error("未找到对应的修改对。");
    }
    session.suggestions[index].decision = "dismissed";
    session.suggestions[index].updatedAt = deps.nowIso();
    deps.updateSessionTimestamp(session);
    return deps.deepClone(session);
  }

  async function deleteSuggestionCommand(sessionId: string, suggestionId: string) {
    const session = deps.getSessionOrThrow(sessionId);
    const index = deps.findSuggestionIndex(session, suggestionId);
    if (index < 0) {
      throw new Error("未找到对应的修改对。");
    }
    const rewriteUnitId = session.suggestions[index].rewriteUnitId;
    session.suggestions.splice(index, 1);
    const stillHasAny = session.suggestions.some(
      (item) => item.rewriteUnitId === rewriteUnitId
    );
    if (!stillHasAny) {
      const unit = session.rewriteUnits.find((item) => item.id === rewriteUnitId);
      if (unit && unit.status === "done") {
        unit.status = "idle";
      }
    }
    deps.updateSessionTimestamp(session);
    return deps.deepClone(session);
  }

  return {
    openDocumentCommand,
    loadSessionCommand,
    resetSessionCommand,
    applySuggestionCommand,
    dismissSuggestionCommand,
    deleteSuggestionCommand
  };
}
