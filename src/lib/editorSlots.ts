import type { DocumentSession, EditorSlotEdit, WritebackSlot } from "./types";

export type EditorSlotOverrides = Record<string, string>;

export function buildEditorTextFromSlots(
  slots: ReadonlyArray<WritebackSlot>,
  overrides: EditorSlotOverrides
) {
  return slots
    .map((slot) => `${resolveEditorSlotText(slot, overrides)}${slot.separatorAfter}`)
    .join("");
}

export function buildEditorTextFromSession(
  session: DocumentSession,
  overrides: EditorSlotOverrides
) {
  return buildEditorTextFromSlots(session.writebackSlots, overrides);
}

export function buildEditorSlotEdits(
  session: DocumentSession,
  overrides: EditorSlotOverrides
): EditorSlotEdit[] {
  return session.writebackSlots
    .filter((slot) => slot.editable)
    .map((slot) => ({
      slotId: slot.id,
      text: resolveEditorSlotText(slot, overrides)
    }));
}

export function resolveEditorSlotText(
  slot: WritebackSlot,
  overrides: EditorSlotOverrides
) {
  return overrides[slot.id] ?? slot.text;
}

export function applyEditorSlotOverride(
  overrides: EditorSlotOverrides,
  slot: WritebackSlot,
  nextText: string
) {
  if (!slot.editable) return overrides;
  if (nextText === slot.text) {
    const { [slot.id]: _removed, ...rest } = overrides;
    return rest;
  }
  return {
    ...overrides,
    [slot.id]: nextText
  };
}
