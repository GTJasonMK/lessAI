import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  DocumentSession,
  DocumentSnapshot,
  EditorSlotEdit,
  ProviderCheckResult,
  RewriteMode
} from "./types";

type EditorWritebackMode = "validate" | "write";
export type EditorWritebackInput =
  | { kind: "text"; content: string }
  | { kind: "slotEdits"; edits: EditorSlotEdit[] };
export type WindowResizeDirection =
  | "East"
  | "North"
  | "NorthEast"
  | "NorthWest"
  | "South"
  | "SouthEast"
  | "SouthWest"
  | "West";

type CommandPayload = Record<string, unknown>;

function invokeCommand<T>(command: string, payload?: CommandPayload) {
  return invoke<T>(command, payload);
}

function invokeSessionCommand<T>(
  command: string,
  sessionId: string,
  payload?: CommandPayload
) {
  return invokeCommand<T>(command, { sessionId, ...payload });
}

export async function loadSettings() {
  return invokeCommand<AppSettings>("load_settings");
}

export async function saveSettings(settings: AppSettings) {
  return invokeCommand<AppSettings>("save_settings", { settings });
}

export async function testProvider(settings: AppSettings) {
  return invokeCommand<ProviderCheckResult>("test_provider", { settings });
}

export async function openDocument(path: string) {
  return invokeCommand<DocumentSession>("open_document", { path });
}

export async function loadSession(sessionId: string) {
  return invokeSessionCommand<DocumentSession>("load_session", sessionId);
}

export async function resetSession(sessionId: string) {
  return invokeSessionCommand<DocumentSession>("reset_session", sessionId);
}

export async function startRewrite(
  sessionId: string,
  mode: RewriteMode,
  targetRewriteUnitIds?: string[]
) {
  return invokeSessionCommand<DocumentSession>("start_rewrite", sessionId, {
    mode,
    targetRewriteUnitIds
  });
}

export async function pauseRewrite(sessionId: string) {
  return invokeSessionCommand<DocumentSession>("pause_rewrite", sessionId);
}

export async function resumeRewrite(sessionId: string) {
  return invokeSessionCommand<DocumentSession>("resume_rewrite", sessionId);
}

export async function cancelRewrite(sessionId: string) {
  return invokeSessionCommand<DocumentSession>("cancel_rewrite", sessionId);
}

export async function retryRewriteUnit(sessionId: string, rewriteUnitId: string) {
  return invokeSessionCommand<DocumentSession>("retry_rewrite_unit", sessionId, {
    rewriteUnitId
  });
}

export async function applySuggestion(sessionId: string, suggestionId: string) {
  return invokeSessionCommand<DocumentSession>("apply_suggestion", sessionId, { suggestionId });
}

export async function dismissSuggestion(sessionId: string, suggestionId: string) {
  return invokeSessionCommand<DocumentSession>("dismiss_suggestion", sessionId, { suggestionId });
}

export async function deleteSuggestion(sessionId: string, suggestionId: string) {
  return invokeSessionCommand<DocumentSession>("delete_suggestion", sessionId, { suggestionId });
}

export async function exportDocument(sessionId: string, path: string) {
  return invokeSessionCommand<string>("export_document", sessionId, { path });
}

export async function finalizeDocument(sessionId: string) {
  return invokeSessionCommand<string>("finalize_document", sessionId);
}

export async function runDocumentWriteback(
  sessionId: string,
  mode: EditorWritebackMode,
  input: EditorWritebackInput,
  editorBaseSnapshot: DocumentSnapshot | null
) {
  return invokeSessionCommand<DocumentSession>("run_document_writeback", sessionId, {
    mode,
    input,
    editorBaseSnapshot
  });
}

export async function rewriteSelection(
  sessionId: string,
  text: string,
  editorBaseSnapshot: DocumentSnapshot | null
) {
  return invokeSessionCommand<string>("rewrite_selection", sessionId, {
    text,
    editorBaseSnapshot
  });
}

export async function isMainWindowMaximized() {
  return invokeCommand<boolean>("is_main_window_maximized");
}

export async function minimizeMainWindow() {
  return invokeCommand<void>("minimize_main_window");
}

export async function toggleMaximizeMainWindow() {
  return invokeCommand<void>("toggle_maximize_main_window");
}

export async function closeMainWindow() {
  return invokeCommand<void>("close_main_window");
}

export async function startDragMainWindow() {
  return invokeCommand<void>("start_drag_main_window");
}

export async function startResizeMainWindow(direction: WindowResizeDirection) {
  return invokeCommand<void>("start_resize_main_window", { direction });
}
