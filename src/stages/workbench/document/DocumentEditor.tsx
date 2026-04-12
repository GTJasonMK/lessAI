import { forwardRef, memo } from "react";

import { isDocxPath } from "../../../lib/helpers";
import { DocxChunkEditor } from "./DocxChunkEditor";
import { PlainTextDocumentEditor } from "./PlainTextDocumentEditor";
import type { DocumentEditorHandle, DocumentEditorProps } from "./documentEditorTypes";

export type {
  DocumentEditorApplyResult,
  DocumentEditorHandle,
  DocumentEditorPreviewResult,
  DocumentEditorSelectionSnapshot,
} from "./documentEditorTypes";

export const DocumentEditor = memo(
  forwardRef<DocumentEditorHandle, DocumentEditorProps>(function DocumentEditor(props, ref) {
    if (isDocxPath(props.session.documentPath)) {
      return <DocxChunkEditor ref={ref} {...props} />;
    }

    return <PlainTextDocumentEditor ref={ref} {...props} />;
  })
);
