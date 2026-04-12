import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  DocumentSession,
  EditorChunkEdit,
  ProviderCheckResult,
  RewriteMode
} from "./types";

export async function loadSettings() {
  return invoke<AppSettings>("load_settings");
}

export async function saveSettings(settings: AppSettings) {
  return invoke<AppSettings>("save_settings", { settings });
}

export async function testProvider(settings: AppSettings) {
  return invoke<ProviderCheckResult>("test_provider", { settings });
}

export async function openDocument(path: string) {
  return invoke<DocumentSession>("open_document", { path });
}

export async function loadSession(sessionId: string) {
  return invoke<DocumentSession>("load_session", { sessionId });
}

export async function resetSession(sessionId: string) {
  return invoke<DocumentSession>("reset_session", { sessionId });
}

export async function startRewrite(
  sessionId: string,
  mode: RewriteMode,
  targetChunkIndices?: number[]
) {
  return invoke<DocumentSession>("start_rewrite", {
    sessionId,
    mode,
    targetChunkIndices
  });
}

export async function pauseRewrite(sessionId: string) {
  return invoke<DocumentSession>("pause_rewrite", { sessionId });
}

export async function resumeRewrite(sessionId: string) {
  return invoke<DocumentSession>("resume_rewrite", { sessionId });
}

export async function cancelRewrite(sessionId: string) {
  return invoke<DocumentSession>("cancel_rewrite", { sessionId });
}

export async function retryChunk(sessionId: string, index: number) {
  return invoke<DocumentSession>("retry_chunk", { sessionId, index });
}

export async function applySuggestion(sessionId: string, suggestionId: string) {
  return invoke<DocumentSession>("apply_suggestion", { sessionId, suggestionId });
}

export async function dismissSuggestion(sessionId: string, suggestionId: string) {
  return invoke<DocumentSession>("dismiss_suggestion", { sessionId, suggestionId });
}

export async function deleteSuggestion(sessionId: string, suggestionId: string) {
  return invoke<DocumentSession>("delete_suggestion", { sessionId, suggestionId });
}

export async function exportDocument(sessionId: string, path: string) {
  return invoke<string>("export_document", { sessionId, path });
}

export async function finalizeDocument(sessionId: string) {
  return invoke<string>("finalize_document", { sessionId });
}

export async function saveDocumentEdits(sessionId: string, content: string) {
  return invoke<DocumentSession>("save_document_edits", { sessionId, content });
}

export async function validateDocumentEdits(sessionId: string, content: string) {
  return invoke<void>("validate_document_edits", { sessionId, content });
}

export async function validateDocumentChunkEdits(sessionId: string, edits: EditorChunkEdit[]) {
  return invoke<void>("validate_document_chunk_edits", { sessionId, edits });
}

export async function saveDocumentChunkEdits(sessionId: string, edits: EditorChunkEdit[]) {
  return invoke<DocumentSession>("save_document_chunk_edits", { sessionId, edits });
}

export async function rewriteSnippet(sessionId: string, text: string) {
  return invoke<string>("rewrite_snippet", { sessionId, text });
}
