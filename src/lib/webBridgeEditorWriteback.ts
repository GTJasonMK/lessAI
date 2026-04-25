import { applySlotUpdates, ensureSessionCanUseEditorWriteback } from "./webBridgeSessionUtils";
import { normalizeTextAgainstSourceLayout } from "./webBridgeText";
import type { DocumentSession, EditorSlotEdit, WritebackSlot } from "./types";

function collectSlotEditUpdates(session: DocumentSession, edits: EditorSlotEdit[]) {
  const editableSlotIds = new Set(
    session.writebackSlots
      .filter((slot) => slot.editable)
      .map((slot) => slot.id)
  );
  if (edits.length !== editableSlotIds.size) {
    throw new Error("编辑器提交的可编辑槽位数量与当前会话不一致，请重新进入编辑模式。");
  }

  const seen = new Set<string>();
  for (const edit of edits) {
    if (!editableSlotIds.has(edit.slotId)) {
      throw new Error(`编辑器提交了不可编辑或不存在的槽位 ${edit.slotId}, 无法安全写回。`);
    }
    if (seen.has(edit.slotId)) {
      throw new Error(`编辑器提交了重复的槽位 ${edit.slotId}, 无法安全写回。`);
    }
    seen.add(edit.slotId);
  }

  return edits.map((edit) => ({
    slotId: edit.slotId,
    text: edit.text
  }));
}

export type EditorWritebackPayload =
  | { kind: "text"; text: string }
  | { kind: "slots"; slots: WritebackSlot[] };

export function buildEditorWritebackPayload(
  session: DocumentSession,
  input: { kind: "text"; content: string } | { kind: "slotEdits"; edits: EditorSlotEdit[] }
): EditorWritebackPayload {
  ensureSessionCanUseEditorWriteback(session);

  if (input.kind === "text") {
    if (!input.content.trim()) {
      throw new Error("文档内容为空，无法保存。");
    }
    switch (session.capabilities.editorMode) {
      case "fullText":
        return {
          kind: "text",
          text: normalizeTextAgainstSourceLayout(session.sourceText, input.content)
        };
      case "slotBased":
        throw new Error("结构化编辑模式必须按槽位保存，不能再走整篇纯文本写回。");
      default:
        throw new Error("当前文档暂不支持整篇纯文本编辑写回。");
    }
  }

  if (session.capabilities.editorMode !== "slotBased") {
    throw new Error("当前仅槽位编辑文档支持按槽位写回。");
  }
  const updates = collectSlotEditUpdates(session, input.edits);
  return {
    kind: "slots",
    slots: applySlotUpdates(session.writebackSlots, updates)
  };
}
