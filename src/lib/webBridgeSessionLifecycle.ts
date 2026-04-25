import {
  buildStructure,
  normalizeNewlines,
  normalizeText,
  simpleHash,
  snapshotFromText
} from "./webBridgeText";
import type { AppSettings, CapabilityGate, DocumentSession } from "./types";

function capabilityGate(allowed: boolean, blockReason?: string | null): CapabilityGate {
  return {
    allowed,
    blockReason: allowed ? null : blockReason ?? "当前文档能力状态不一致，缺少阻断原因。"
  };
}

function normalizeCapabilityPolicyGate(gate: CapabilityGate | null | undefined): CapabilityGate {
  if (!gate) {
    return capabilityGate(true);
  }
  if (gate.allowed || gate.blockReason) {
    return capabilityGate(gate.allowed, gate.blockReason);
  }
  return capabilityGate(true);
}

function sameCapabilityGate(left: CapabilityGate, right: CapabilityGate) {
  return left.allowed === right.allowed && left.blockReason === right.blockReason;
}

interface SessionLifecycleDeps {
  sessions: Map<string, DocumentSession>;
  nowIso: () => string;
  getSettings: () => AppSettings;
  getVirtualFile: (path: string) => { text: string } | null;
  snapshotMismatchError: string;
  aiRewriteBlockReason: string;
  dirtySessionBlockReason: string;
}

export function createSessionLifecycle(deps: SessionLifecycleDeps) {
  function isSessionClean(session: DocumentSession) {
    return (
      session.status === "idle" &&
      session.suggestions.length === 0 &&
      session.rewriteUnits.every((unit) => unit.status === "idle" || unit.status === "done")
    );
  }

  function hydrateCapabilities(session: DocumentSession) {
    const cleanSession = isSessionClean(session);
    const sourceWriteback = normalizeCapabilityPolicyGate(session.capabilities?.sourceWriteback);
    const editorWriteback = normalizeCapabilityPolicyGate(session.capabilities?.editorWriteback);
    const editorMode = editorWriteback.allowed ? "fullText" : "none";
    const aiRewrite = sourceWriteback.allowed
      ? capabilityGate(true)
      : capabilityGate(false, sourceWriteback.blockReason ?? deps.aiRewriteBlockReason);
    const editorEntry =
      editorMode === "none"
        ? capabilityGate(false, editorWriteback.blockReason ?? "当前文档暂不支持进入编辑模式。")
        : cleanSession
          ? capabilityGate(true)
          : capabilityGate(false, deps.dirtySessionBlockReason);

    session.capabilities = {
      backendKind: "textual",
      editorMode,
      cleanSession,
      sourceWriteback,
      aiRewrite,
      editorWriteback,
      editorEntry
    };
  }

  function extractFileTitle(path: string) {
    const parts = path.split("/");
    const filename = parts[parts.length - 1] ?? "untitled.txt";
    const decoded = decodeURIComponent(filename);
    const idx = decoded.lastIndexOf(".");
    return idx > 0 ? decoded.slice(0, idx) : decoded;
  }

  function sessionIdFromPath(path: string) {
    return `session-${simpleHash(path)}`;
  }

  function buildCleanSession(params: {
    id: string;
    path: string;
    title: string;
    sourceText: string;
    settings: AppSettings;
    createdAt?: string;
  }) {
    const now = deps.nowIso();
    const sourceText = normalizeNewlines(params.sourceText);
    const structure = buildStructure(sourceText, params.settings.segmentationPreset);
    const session: DocumentSession = {
      id: params.id,
      title: params.title,
      documentPath: params.path,
      sourceText,
      sourceSnapshot: snapshotFromText(sourceText),
      templateKind: "plain_text",
      templateSignature: null,
      slotStructureSignature: null,
      normalizedText: normalizeText(sourceText),
      capabilities: {
        backendKind: "textual",
        editorMode: "fullText",
        cleanSession: true,
        sourceWriteback: capabilityGate(true),
        aiRewrite: capabilityGate(true),
        editorWriteback: capabilityGate(true),
        editorEntry: capabilityGate(true)
      },
      segmentationPreset: params.settings.segmentationPreset,
      rewriteHeadings: params.settings.rewriteHeadings,
      writebackSlots: structure.writebackSlots,
      rewriteUnits: structure.rewriteUnits,
      suggestions: [],
      nextSuggestionSequence: 1,
      status: "idle",
      createdAt: params.createdAt ?? now,
      updatedAt: now
    };
    hydrateCapabilities(session);
    return session;
  }

  function getSessionOrThrow(sessionId: string) {
    const session = deps.sessions.get(sessionId);
    if (!session) {
      throw new Error(`未找到会话：${sessionId}`);
    }
    return session;
  }

  function blockSessionForExternalChange(session: DocumentSession, reason: string) {
    const nextSourceWriteback = capabilityGate(false, reason);
    const nextEditorWriteback = capabilityGate(false, reason);
    const sourceChanged = !sameCapabilityGate(
      normalizeCapabilityPolicyGate(session.capabilities?.sourceWriteback),
      nextSourceWriteback
    );
    const editorChanged = !sameCapabilityGate(
      normalizeCapabilityPolicyGate(session.capabilities?.editorWriteback),
      nextEditorWriteback
    );

    session.capabilities.sourceWriteback = nextSourceWriteback;
    session.capabilities.editorWriteback = nextEditorWriteback;
    hydrateCapabilities(session);
    if (sourceChanged || editorChanged) {
      session.updatedAt = deps.nowIso();
    }
  }

  async function loadSessionInternal(sessionId: string) {
    const session = getSessionOrThrow(sessionId);
    const settings = deps.getSettings();
    const file = deps.getVirtualFile(session.documentPath);
    if (!file) {
      return session;
    }

    const normalizedFileText = normalizeNewlines(file.text);
    const sourceChanged = session.sourceText !== normalizedFileText;
    if (sourceChanged) {
      if (isSessionClean(session)) {
        const rebuilt = buildCleanSession({
          id: session.id,
          path: session.documentPath,
          title: session.title,
          sourceText: file.text,
          settings,
          createdAt: session.createdAt
        });
        deps.sessions.set(session.id, rebuilt);
        return rebuilt;
      }
      blockSessionForExternalChange(session, deps.snapshotMismatchError);
      return session;
    }

    const shouldRebuild =
      isSessionClean(session) &&
      (session.segmentationPreset !== settings.segmentationPreset ||
        session.rewriteHeadings !== settings.rewriteHeadings);

    if (shouldRebuild) {
      const rebuilt = buildCleanSession({
        id: session.id,
        path: session.documentPath,
        title: session.title,
        sourceText: file.text,
        settings,
        createdAt: session.createdAt
      });
      deps.sessions.set(session.id, rebuilt);
      return rebuilt;
    }

    hydrateCapabilities(session);
    return session;
  }

  return {
    hydrateCapabilities,
    extractFileTitle,
    sessionIdFromPath,
    buildCleanSession,
    loadSessionInternal
  };
}
