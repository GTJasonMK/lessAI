import { forwardRef, memo } from "react";

import { documentEditorMode } from "../../../lib/documentCapabilities";
import { PlainTextDocumentEditor } from "./PlainTextDocumentEditor";
import { StructuredSlotEditor } from "./StructuredSlotEditor";
import type { DocumentEditorHandle, DocumentEditorProps } from "./documentEditorTypes";

export type {
  DocumentEditorApplyResult,
  DocumentEditorHandle,
  DocumentEditorPreviewResult,
  DocumentEditorSelectionSnapshot,
} from "./documentEditorTypes";

export const DocumentEditor = memo(
  forwardRef<DocumentEditorHandle, DocumentEditorProps>(function DocumentEditor(props, ref) {
    if (documentEditorMode(props.session) === "slotBased") {
      return <StructuredSlotEditor ref={ref} {...props} />;
    }

    return <PlainTextDocumentEditor ref={ref} {...props} />;
  })
);
