import type {
  DocumentBackendKind,
  DocumentEditorMode,
  DocumentSession
} from "./types";

export function documentBackendKind(session: DocumentSession): DocumentBackendKind {
  return session.capabilities.backendKind;
}

export function documentEditorMode(session: DocumentSession): DocumentEditorMode {
  const raw = (session.capabilities as { editorMode?: string | null }).editorMode ?? "";
  switch (raw) {
    case "slotBased":
    case "slotbased":
      return "slotBased";
    case "fullText":
    case "fulltext":
      return "fullText";
    default:
      return "none";
  }
}

export function sessionSupportsSourceWriteback(session: DocumentSession) {
  return session.capabilities.sourceWriteback.allowed;
}

export function sessionSupportsAiRewrite(session: DocumentSession) {
  return session.capabilities.aiRewrite.allowed;
}

export function sessionSupportsEditorEntry(session: DocumentSession) {
  return session.capabilities.editorEntry.allowed;
}

export function editorEntryBlockedReason(session: DocumentSession) {
  return session.capabilities.editorEntry.blockReason;
}

export function sessionIsClean(session: DocumentSession) {
  return session.capabilities.cleanSession;
}
