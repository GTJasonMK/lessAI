import { Fragment, memo } from "react";

import type { EditorSlotOverrides } from "../../../lib/editorSlots";
import { resolveEditorSlotText } from "../../../lib/editorSlots";
import { resolveRewriteUnitSlots } from "../../../lib/helpers";
import type { DocumentSession, RewriteUnit } from "../../../lib/types";
import { EditableSlotSpan, slotPresentationClass } from "./structuredEditorShared";

interface StructuredEditorUnitProps {
  session: DocumentSession;
  rewriteUnit: RewriteUnit;
  slotOverrides: EditorSlotOverrides;
  busy: boolean;
  registerNode: (slotId: string, node: HTMLSpanElement | null) => void;
  onChangeSlotText: (slotId: string, value: string) => void;
}

function buildUnitClassName(hasEditableSlot: boolean) {
  return ["structured-editor-unit", hasEditableSlot ? "" : "is-protected"]
    .filter(Boolean)
    .join(" ");
}

export const StructuredEditorUnit = memo(function StructuredEditorUnit({
  session,
  rewriteUnit,
  slotOverrides,
  busy,
  registerNode,
  onChangeSlotText
}: StructuredEditorUnitProps) {
  const slots = resolveRewriteUnitSlots(session, rewriteUnit);
  if (slots.length === 0) {
    return null;
  }

  const hasEditableSlot = slots.some((slot) => slot.editable);
  const trailingSeparator = slots[slots.length - 1]?.separatorAfter ?? "";

  return (
    <Fragment>
      <span className={buildUnitClassName(hasEditableSlot)} data-rewrite-unit-id={rewriteUnit.id}>
        {slots.map((slot, index) => {
          const text = resolveEditorSlotText(slot, slotOverrides);
          const intraUnitSeparator = index < slots.length - 1 ? slot.separatorAfter : "";
          return (
            <Fragment key={slot.id}>
              {slot.editable ? (
                <EditableSlotSpan
                  slot={slot}
                  text={text}
                  busy={busy}
                  registerNode={registerNode}
                  onChange={onChangeSlotText}
                />
              ) : (
                <span className={slotPresentationClass(slot)}>{text}</span>
              )}
              {intraUnitSeparator}
            </Fragment>
          );
        })}
      </span>
      {trailingSeparator}
    </Fragment>
  );
});
