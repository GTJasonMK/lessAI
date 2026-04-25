import { sanitizeFileName } from "./webBridgeModelApi";
import { buildAppliedProjection, downloadTextFile, mergedTextFromSlots } from "./webBridgeSessionUtils";
import { normalizeTextAgainstSourceLayout } from "./webBridgeText";
import type { EditorWritebackPayload } from "./webBridgeEditorWriteback";
import type { AppSettings, DocumentSession, DocumentSnapshot, EditorSlotEdit } from "./types";

interface BuildCleanSessionInput {
  id: string;
  path: string;
  title: string;
  sourceText: string;
  settings: AppSettings;
  createdAt?: string;
}

interface WritebackCommandDeps {
  sessions: Map<string, DocumentSession>;
  deepClone: <T>(value: T) => T;
  getSettings: () => AppSettings;
  getSessionOrThrow: (sessionId: string) => DocumentSession;
  ensureNoActiveJob: (sessionId: string, errorMessage: string) => void;
  ensureEditorBaseSnapshotMatches: (
    session: DocumentSession,
    editorBaseSnapshot: DocumentSnapshot | null | undefined
  ) => void;
  ensureSessionSourceMatches: (session: DocumentSession) => void;
  buildEditorWritebackPayload: (
    session: DocumentSession,
    input: { kind: "text"; content: string } | { kind: "slotEdits"; edits: EditorSlotEdit[] }
  ) => EditorWritebackPayload;
  updateVirtualFileText: (path: string, text: string) => void;
  buildCleanSession: (params: BuildCleanSessionInput) => DocumentSession;
  saveFinalizeRecord: (record: {
    sessionId: string;
    documentPath: string;
    title: string;
    beforeText: string;
    afterText: string;
  }) => void;
  activeEditorSessionError: string;
  activeJobFinalizeError: string;
}

export function createWritebackCommands(deps: WritebackCommandDeps) {
  async function runDocumentWritebackCommand(params: {
    sessionId: string;
    mode: "validate" | "write";
    editorBaseSnapshot?: DocumentSnapshot | null;
    input:
      | { kind: "text"; content: string }
      | { kind: "slotEdits"; edits: EditorSlotEdit[] };
  }) {
    const session = deps.getSessionOrThrow(params.sessionId);
    deps.ensureNoActiveJob(params.sessionId, deps.activeEditorSessionError);
    deps.ensureEditorBaseSnapshotMatches(session, params.editorBaseSnapshot);
    deps.ensureSessionSourceMatches(session);
    const payload = deps.buildEditorWritebackPayload(session, params.input);

    if (params.mode === "validate") {
      return deps.deepClone(session);
    }

    const nextText =
      payload.kind === "text"
        ? payload.text
        : mergedTextFromSlots(payload.slots);
    deps.updateVirtualFileText(session.documentPath, nextText);
    const rebuilt = deps.buildCleanSession({
      id: session.id,
      path: session.documentPath,
      title: session.title,
      sourceText: nextText,
      settings: deps.getSettings(),
      createdAt: session.createdAt
    });
    deps.sessions.set(session.id, rebuilt);
    return deps.deepClone(rebuilt);
  }

  async function exportDocumentCommand(sessionId: string, path: string) {
    const session = deps.getSessionOrThrow(sessionId);
    const merged = normalizeTextAgainstSourceLayout(
      session.sourceText,
      mergedTextFromSlots(buildAppliedProjection(session))
    );
    const filename = sanitizeFileName(path || `${session.title}.txt`);
    downloadTextFile(filename, merged);
    return filename;
  }

  async function finalizeDocumentCommand(sessionId: string) {
    deps.ensureNoActiveJob(sessionId, deps.activeJobFinalizeError);
    const session = deps.getSessionOrThrow(sessionId);
    deps.ensureSessionSourceMatches(session);
    const beforeText = session.sourceText;
    const afterText = normalizeTextAgainstSourceLayout(
      beforeText,
      mergedTextFromSlots(buildAppliedProjection(session))
    );

    deps.saveFinalizeRecord({
      sessionId,
      documentPath: session.documentPath,
      title: session.title,
      beforeText,
      afterText
    });
    deps.updateVirtualFileText(session.documentPath, afterText);
    deps.sessions.delete(sessionId);
    return session.documentPath;
  }

  return {
    runDocumentWritebackCommand,
    exportDocumentCommand,
    finalizeDocumentCommand
  };
}
