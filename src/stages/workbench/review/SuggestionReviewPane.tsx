import { memo, useMemo } from "react";
import { AlertCircle } from "lucide-react";
import type { ChunkTask, DocumentSession, EditSuggestion } from "../../../lib/types";
import type { SessionStats } from "../../../lib/helpers";
import type { ReviewView } from "../../../lib/constants";
import { REVIEW_VIEW_OPTIONS } from "../../../lib/constants";
import { guessClientDocumentFormat, renderInlineProtectedText } from "../../../lib/protectedText";
import {
  countCharacters,
  formatDate,
  formatSuggestionDecision,
  getLatestSuggestion,
  suggestionTone
} from "../../../lib/helpers";
import { StatusBadge } from "../../../components/StatusBadge";

interface SuggestionReviewPaneProps {
  currentSession: DocumentSession;
  currentStats: SessionStats;
  activeChunk: ChunkTask | null;
  activeSuggestionId: string | null;
  activeSuggestion: EditSuggestion | null;
  showMarkers: boolean;
  reviewView: ReviewView;
  orderedSuggestions: EditSuggestion[];
  onSetReviewView: (view: ReviewView) => void;
  onSelectChunk: (index: number) => void;
  onSelectSuggestion: (suggestionId: string) => void;
}

export const SuggestionReviewPane = memo(function SuggestionReviewPane({
  currentSession,
  currentStats,
  activeChunk,
  activeSuggestionId,
  activeSuggestion,
  showMarkers,
  reviewView,
  orderedSuggestions,
  onSetReviewView,
  onSelectChunk,
  onSelectSuggestion
}: SuggestionReviewPaneProps) {
  const documentFormat = useMemo(
    () => guessClientDocumentFormat(currentSession.documentPath),
    [currentSession.documentPath]
  );
  const latestSuggestion = useMemo(() => getLatestSuggestion(currentSession), [currentSession]);
  const activeCandidateCharacters = activeSuggestion?.afterText
    ? countCharacters(activeSuggestion.afterText)
    : 0;

  const renderText = (value: string, key: string) => {
    if (!showMarkers) return value;
    return renderInlineProtectedText(value, documentFormat, key);
  };

  return (
    <>
      <div className="context-group">
        <span className="context-chip">修改对：{currentStats.suggestionsTotal}</span>
        <span className="context-chip">待审阅：{currentStats.suggestionsProposed}</span>
        <span className="context-chip">
          已应用：{currentStats.chunksApplied}/{currentStats.total}
        </span>
        <span className="context-chip">候选稿：{activeCandidateCharacters} 字</span>
        <span className="context-chip">
          {activeSuggestion
            ? `当前 #${activeSuggestion.sequence}`
            : latestSuggestion
              ? `最新 #${latestSuggestion.sequence}`
              : "暂无修改对"}
        </span>
      </div>

      {activeSuggestion ? (
        <div className="review-switches">
          {REVIEW_VIEW_OPTIONS.map((item) => (
            <button
              key={item.key}
              type="button"
              className={`switch-chip ${reviewView === item.key ? "is-active" : ""}`}
              onClick={() => onSetReviewView(item.key)}
            >
              {item.label}
            </button>
          ))}
        </div>
      ) : null}

      {activeChunk?.status === "failed" ? (
        <div className="error-card">
          <AlertCircle />
          <div>
            <strong>该片段生成失败</strong>
            <span>{activeChunk.errorMessage ?? "请点击重试重新生成。"}</span>
          </div>
        </div>
      ) : null}

      {activeSuggestion ? (
        <div className="diff-view">
          {reviewView === "diff" ? (
            activeSuggestion.diffSpans.length > 0 ? (
              <p>
                {activeSuggestion.diffSpans.map((span, index) => (
                  <span
                    key={`${span.type}-${index}-${span.text.length}`}
                    className={`diff-span is-${span.type}`}
                  >
                    {renderText(
                      span.text,
                      `suggestion-${activeSuggestion.id}-diff-${span.type}-${index}`
                    )}
                  </span>
                ))}
              </p>
            ) : (
              <div className="empty-inline">
                <span>该修改对没有可展示的 diff。</span>
              </div>
            )
          ) : null}

          {reviewView === "source" ? (
            <p>
              {renderText(
                activeSuggestion.beforeText,
                `suggestion-${activeSuggestion.id}-source`
              )}
            </p>
          ) : null}

          {reviewView === "candidate" ? (
            <p>
              {renderText(
                activeSuggestion.afterText,
                `suggestion-${activeSuggestion.id}-candidate`
              )}
            </p>
          ) : null}
        </div>
      ) : (
        <div className="empty-inline">
          <span>点击下方任意修改对查看细节。</span>
        </div>
      )}

      <div className="suggestion-list scroll-region">
        {orderedSuggestions.length === 0 ? (
          <div className="empty-inline">
            <span>还没有修改对。点击左侧「文档」右上角的“开始优化”生成一段。</span>
          </div>
        ) : (
          orderedSuggestions.map((suggestion) => (
            <button
              key={suggestion.id}
              type="button"
              className={`suggestion-row ${suggestion.id === activeSuggestionId ? "is-active" : ""}`}
              onClick={() => {
                onSelectChunk(suggestion.chunkIndex);
                onSelectSuggestion(suggestion.id);
              }}
            >
              <div className="suggestion-row-head">
                <strong>
                  #{suggestion.sequence} ·{" "}
                  {suggestion.beforeText.trim().replace(/\s+/g, " ").slice(0, 24) ||
                    "（空片段）"}
                  {suggestion.beforeText.trim().length > 24 ? "…" : ""}
                </strong>
                <StatusBadge tone={suggestionTone(suggestion.decision)}>
                  {formatSuggestionDecision(suggestion.decision)}
                </StatusBadge>
              </div>
              <div className="suggestion-row-meta">
                <span>{formatDate(suggestion.createdAt)}</span>
                <span>{countCharacters(suggestion.afterText)} 字</span>
              </div>
              <p className="suggestion-row-preview">{suggestion.afterText}</p>
            </button>
          ))
        )}
      </div>
    </>
  );
});
