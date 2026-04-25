import { type CompletedRewriteUnitPayload } from "./webBridgeAutoJobRuntime";
import { callChatModel, ensureSettingsReady } from "./webBridgeModelApi";
import {
  REWRITE_UNIT_NOT_FOUND_ERROR,
  buildRewriteBatchRequest,
  buildRewriteUnitRequest,
  parseRewriteBatchResponse,
  rewriteBatchSystemPrompt,
  rewriteBatchUserPrompt
} from "./webBridgeProtocol";
import {
  createRewriteSuggestion,
  protectedRewriteUnitError,
  validateCandidateBatchWriteback
} from "./webBridgeRewriteHelpers";
import { applySuggestionById, buildAppliedProjection, rewriteUnitSourceText } from "./webBridgeSessionUtils";
import type { AppSettings, CapabilityGate, DocumentSession } from "./types";

interface RewritePipelineDeps {
  getSettings: () => AppSettings;
  ensureCapabilityAllowed: (gate: CapabilityGate, fallbackMessage: string) => void;
  ensureSessionSourceMatches: (session: DocumentSession) => void;
  aiRewriteBlockReason: string;
  deepClone: <T>(value: T) => T;
  updateSessionTimestamp: (session: DocumentSession) => void;
  nowIso: () => string;
  rewriteUnitRiskWarningNonWhitespaceChars: number;
}

export function createRewritePipeline(deps: RewritePipelineDeps) {
  function ensureSessionCanRewrite(session: DocumentSession) {
    deps.ensureCapabilityAllowed(session.capabilities.aiRewrite, deps.aiRewriteBlockReason);
    deps.ensureSessionSourceMatches(session);
  }

  function validateSessionWriteback(session: DocumentSession) {
    ensureSessionCanRewrite(session);
    buildAppliedProjection(session);
  }

  async function processRewriteBatch(params: {
    session: DocumentSession;
    rewriteUnitIds: string[];
    autoApprove: boolean;
    signal?: AbortSignal;
  }): Promise<CompletedRewriteUnitPayload[]> {
    const settings = deps.getSettings();
    ensureSettingsReady(settings);
    ensureSessionCanRewrite(params.session);
    const completed: CompletedRewriteUnitPayload[] = [];

    if (params.rewriteUnitIds.length === 0) {
      return completed;
    }

    const unitRequests = params.rewriteUnitIds.map((rewriteUnitId) =>
      buildRewriteUnitRequest(params.session, rewriteUnitId)
    );
    for (const request of unitRequests) {
      if (!request.slots.some((slot) => slot.editable)) {
        throw new Error(protectedRewriteUnitError(request.rewriteUnitId));
      }
    }

    for (const rewriteUnitId of params.rewriteUnitIds) {
      const rewriteUnit = params.session.rewriteUnits.find((item) => item.id === rewriteUnitId);
      if (!rewriteUnit) {
        throw new Error(REWRITE_UNIT_NOT_FOUND_ERROR);
      }
      const riskyChars = rewriteUnitSourceText(params.session, rewriteUnit)
        .replace(/\s+/g, "")
        .length;
      if (riskyChars >= deps.rewriteUnitRiskWarningNonWhitespaceChars) {
        // Keep behavior consistent with front-end warnings; backend itself does not block.
        // No-op here.
      }
    }

    const batchRequest = buildRewriteBatchRequest(unitRequests);
    const raw = await callChatModel(
      settings,
      rewriteBatchSystemPrompt(),
      rewriteBatchUserPrompt(batchRequest),
      params.signal
    );
    const batchResponse = parseRewriteBatchResponse(batchRequest, raw);
    validateCandidateBatchWriteback(
      params.session,
      batchResponse.results,
      deps.deepClone,
      validateSessionWriteback
    );

    for (const response of batchResponse.results) {
      const suggestion = createRewriteSuggestion(
        params.session,
        response,
        params.autoApprove ? "applied" : "proposed"
      );
      params.session.suggestions.push(suggestion);
      if (params.autoApprove) {
        applySuggestionById(params.session, suggestion.id, deps.nowIso());
      }
      const rewriteUnit = params.session.rewriteUnits.find(
        (item) => item.id === response.rewriteUnitId
      );
      if (!rewriteUnit) {
        throw new Error(REWRITE_UNIT_NOT_FOUND_ERROR);
      }
      rewriteUnit.status = "done";
      rewriteUnit.errorMessage = null;
      completed.push({
        rewriteUnitId: response.rewriteUnitId,
        suggestionId: suggestion.id,
        suggestionSequence: suggestion.sequence
      });
    }

    deps.updateSessionTimestamp(params.session);
    return completed;
  }

  return {
    ensureSessionCanRewrite,
    validateSessionWriteback,
    processRewriteBatch
  };
}
