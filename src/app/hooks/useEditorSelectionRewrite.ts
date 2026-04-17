import { useCallback } from "react";
import type { MutableRefObject } from "react";
import { rewriteSelection, runDocumentWriteback } from "../../lib/api";
import type { DocumentSession, DocumentSnapshot } from "../../lib/types";
import { countCharacters, readableError } from "../../lib/helpers";
import type { ConfirmModalOptions } from "../../components/ConfirmModal";
import type { DocumentEditorHandle } from "../../stages/workbench/document/DocumentEditor";
import type { ShowNotice, WithBusy } from "./sessionActionShared";

const SELECTION_RISK_WARNING_NON_WHITESPACE_CHARS = 6000;

function selectionSizeSummary(text: string) {
  const rawChars = text.length;
  const nonWhitespaceChars = countCharacters(text);
  const lineBreaks = text.split(/\r\n|\r|\n/).length - 1;
  return { rawChars, nonWhitespaceChars, lineBreaks };
}

export function useEditorSelectionRewrite(options: {
  stageRef: MutableRefObject<"workbench" | "editor">;
  currentSessionRef: MutableRefObject<DocumentSession | null>;
  editorBaseSnapshotRef: MutableRefObject<DocumentSnapshot | null>;
  editorRef: MutableRefObject<DocumentEditorHandle | null>;
  requestConfirm: (options: ConfirmModalOptions) => Promise<boolean>;
  showNotice: ShowNotice;
  withBusy: WithBusy;
}) {
  const {
    stageRef,
    currentSessionRef,
    editorBaseSnapshotRef,
    editorRef,
    requestConfirm,
    showNotice,
    withBusy
  } = options;

  const confirmIfSelectionTooLarge = useCallback(
    async (text: string) => {
      const size = selectionSizeSummary(text);
      if (size.nonWhitespaceChars < SELECTION_RISK_WARNING_NON_WHITESPACE_CHARS) {
        return true;
      }

      const ok = await requestConfirm({
        title: "选区过长风险提示",
        message: [
          "当前选区较长，可能导致接口报错（上下文超限）或超时。",
          "",
          `非空字符：${size.nonWhitespaceChars.toLocaleString()}（经验阈值 ${SELECTION_RISK_WARNING_NON_WHITESPACE_CHARS.toLocaleString()}）`,
          `总字符：${size.rawChars.toLocaleString()}`,
          `换行数：${size.lineBreaks.toLocaleString()}`,
          "",
          "系统不会替你自动拆分选区。选择继续将按当前选区直接调用模型。"
        ].join("\n"),
        okLabel: "继续处理",
        cancelLabel: "取消",
        variant: "primary"
      });
      return ok;
    },
    [requestConfirm]
  );

  const handleRewriteSelection = useCallback(async () => {
    if (stageRef.current !== "editor") {
      showNotice("warning", "该功能仅在“编辑终稿”中可用。");
      return;
    }

    const session = currentSessionRef.current;
    if (!session) {
      showNotice("warning", "请先打开一个文档。");
      return;
    }

    const editor = editorRef.current;
    if (!editor) {
      showNotice("warning", "编辑器尚未就绪，请稍后再试。");
      return;
    }

    const snapshot = editor.captureSelection();
    if (!snapshot) {
      showNotice("warning", "请先在正文中选中需要处理的文本。");
      return;
    }

    const ok = await confirmIfSelectionTooLarge(snapshot.text);
    if (!ok) {
      showNotice("info", "已取消处理。");
      return;
    }

    try {
      const rewritten = await withBusy("rewrite-selection", () =>
        rewriteSelection(session.id, snapshot.text, editorBaseSnapshotRef.current)
      );
      const preview = editor.previewSelectionReplacement(snapshot, rewritten);
      if (!preview.ok) {
        showNotice("warning", preview.error);
        return;
      }

      await withBusy("validate-document-edits", () => {
        const input = preview.slotEdits
          ? { kind: "slotEdits" as const, edits: preview.slotEdits }
          : { kind: "text" as const, content: preview.value };
        return runDocumentWriteback(
          session.id,
          "validate",
          input,
          editorBaseSnapshotRef.current
        );
      });

      const applied = editor.applySelectionReplacement(snapshot, rewritten);
      if (!applied.ok) {
        showNotice("warning", applied.error);
        return;
      }

      showNotice("success", "已完成：对选区执行降 AIGC 处理。");
    } catch (error) {
      showNotice("error", `执行失败：${readableError(error)}`);
    }
  }, [
    confirmIfSelectionTooLarge,
    currentSessionRef,
    editorBaseSnapshotRef,
    editorRef,
    showNotice,
    stageRef,
    withBusy
  ]);

  return { handleRewriteSelection } as const;
}
