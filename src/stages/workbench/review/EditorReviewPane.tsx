import { memo, useEffect, useRef, useState } from "react";
import { countCharacters } from "../../../lib/helpers";
import { StatusBadge } from "../../../components/StatusBadge";
import type { DocumentSession } from "../../../lib/types";
import { useEditorHunks } from "../hooks/useEditorHunks";

type EditorReviewView = "diff" | "source" | "current";

const EDITOR_REVIEW_OPTIONS: ReadonlyArray<{
  key: EditorReviewView;
  label: string;
}> = [
  { key: "diff", label: "Diff" },
  { key: "source", label: "原文" },
  { key: "current", label: "当前" }
];

interface EditorReviewPaneProps {
  currentSession: DocumentSession;
  editorText: string;
  editorDirty: boolean;
}

export const EditorReviewPane = memo(function EditorReviewPane({
  currentSession,
  editorText,
  editorDirty
}: EditorReviewPaneProps) {
  const [editorReviewView, setEditorReviewView] = useState<EditorReviewView>("diff");
  const editorDiffViewRef = useRef<HTMLDivElement | null>(null);

  const {
    editorDiffStats,
    editorHunks,
    activeEditorHunk,
    setActiveEditorHunkId
  } = useEditorHunks({
    enabled: true,
    currentSession,
    editorText
  });

  useEffect(() => {
    const node = editorDiffViewRef.current;
    if (!node) return;
    node.scrollTop = 0;
  }, [activeEditorHunk?.id, editorReviewView]);

  return (
    <>
      <div className="context-group">
        <span className="context-chip">手动编辑：{editorDirty ? "未保存" : "已保存"}</span>
        <span className="context-chip">
          变更：+{editorDiffStats.inserted} -{editorDiffStats.deleted}
        </span>
        <span className="context-chip">变更块：{editorHunks.length}</span>
      </div>

      {activeEditorHunk ? (
        <>
          <div className="review-switches">
            {EDITOR_REVIEW_OPTIONS.map((item) => (
              <button
                key={item.key}
                type="button"
                className={`switch-chip ${editorReviewView === item.key ? "is-active" : ""}`}
                onClick={() => setEditorReviewView(item.key)}
              >
                {item.label}
              </button>
            ))}
          </div>

          <div className="diff-view" ref={editorDiffViewRef}>
            {editorReviewView === "diff" ? (
              <p>
                {activeEditorHunk.diffSpans.map((span, index) => (
                  <span
                    key={`${span.type}-${index}-${span.text.length}`}
                    className={`diff-span is-${span.type}`}
                  >
                    {span.text}
                  </span>
                ))}
              </p>
            ) : null}

            {editorReviewView === "source" ? <p>{activeEditorHunk.beforeText}</p> : null}

            {editorReviewView === "current" ? <p>{activeEditorHunk.afterText}</p> : null}
          </div>

          <div className="suggestion-list scroll-region">
            {editorHunks.map((hunk) => {
              const compact = (value: string) => value.replace(/\s+/g, " ").trim();
              const preferred = compact(hunk.afterText) || compact(hunk.beforeText);
              const preview =
                preferred.slice(0, 24) ||
                (hunk.insertedChars === 0 && hunk.deletedChars === 0
                  ? "（仅空白变更）"
                  : "（空变更）");
              const more = preferred.length > 24 ? "…" : "";

              return (
                <button
                  key={hunk.id}
                  type="button"
                  className={`suggestion-row ${hunk.id === activeEditorHunk.id ? "is-active" : ""}`}
                  onClick={() => setActiveEditorHunkId(hunk.id)}
                >
                  <div className="suggestion-row-head">
                    <strong>
                      #{hunk.sequence} · {preview}
                      {more}
                    </strong>
                    <StatusBadge tone="info">
                      +{hunk.insertedChars} -{hunk.deletedChars}
                    </StatusBadge>
                  </div>
                  <div className="suggestion-row-meta">
                    <span>片段：{countCharacters(hunk.afterText)} 字</span>
                  </div>
                  <p className="suggestion-row-preview">{hunk.afterText}</p>
                </button>
              );
            })}
          </div>
        </>
      ) : (
        <div className="empty-inline">
          <span>暂无变更。</span>
        </div>
      )}
    </>
  );
});

