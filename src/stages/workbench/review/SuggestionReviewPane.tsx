import { memo, useMemo } from "react";
import { AlertCircle } from "lucide-react";
import type { DocumentSession, RewriteSuggestion, RewriteUnit } from "../../../lib/types";
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
  activeRewriteUnit: RewriteUnit | null;
  activeSuggestionId: string | null;
  activeSuggestion: RewriteSuggestion | null;
  showMarkers: boolean;
  reviewView: ReviewView;
  orderedSuggestions: RewriteSuggestion[];
  onSetReviewView: (view: ReviewView) => void;
  onSelectRewriteUnit: (rewriteUnitId: string, options?: { multiSelect?: boolean }) => void;
  onSelectSuggestion: (suggestionId: string) => void;
}

export const SuggestionReviewPane = memo(function SuggestionReviewPane({
  currentSession,
  currentStats,
  activeRewriteUnit,
  activeSuggestionId,
  activeSuggestion,
  showMarkers,
  reviewView,
  orderedSuggestions,
  onSetReviewView,
  onSelectRewriteUnit,
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
          已应用：{currentStats.unitsApplied}/{currentStats.total}
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

      {activeRewriteUnit?.status === "failed" ? (
        <div className="error-card">
          <AlertCircle />
          <div>
            <strong>该片段生成失败</strong>
            <span>{activeRewriteUnit.errorMessage ?? "请点击重试重新生成。"}</span>
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
          orderedSuggestions.map((suggestion) => {
            const compact = (value: string) => value.replace(/\s+/g, " ").trim();
            const before = compact(suggestion.beforeText);
            const after = compact(suggestion.afterText);
            const preferred = before || after || "（空片段）";
            const preview = preferred.slice(0, 200);
            const more = preferred.length > 200 ? "…" : "";
            const meta = `${formatDate(suggestion.createdAt)} · ${countCharacters(suggestion.afterText)} 字`;

            return (
              <button
                key={suggestion.id}
                type="button"
                className={`suggestion-row ${suggestion.id === activeSuggestionId ? "is-active" : ""}`}
                onClick={() => {
                  onSelectRewriteUnit(suggestion.rewriteUnitId);
                  onSelectSuggestion(suggestion.id);
                }}
                title={meta}
              >
                <div className="suggestion-row-line">
                  <span className="suggestion-row-title">
                    #{suggestion.sequence} · {preview}
                    {more}
                  </span>
                  <span className="suggestion-row-meta-inline">{meta}</span>
                  <StatusBadge tone={suggestionTone(suggestion.decision)}>
                    {formatSuggestionDecision(suggestion.decision)}
                  </StatusBadge>
                </div>
              </button>
            );
          })
        )}
      </div>
    </>
  );
});
