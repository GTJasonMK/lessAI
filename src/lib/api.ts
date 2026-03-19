import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  DocumentSession,
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

export async function startRewrite(sessionId: string, mode: RewriteMode) {
  return invoke<DocumentSession>("start_rewrite", { sessionId, mode });
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
