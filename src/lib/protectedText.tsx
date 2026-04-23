import type { ReactNode } from "react";

import { splitMarkdownInlineProtected } from "./markdownProtectedSegments";
import { fileExtensionLower } from "./path";
import { type ProtectedSegment } from "./protectedTextShared";
import { splitTexInlineProtected } from "./texProtectedSegments";

export type ClientDocumentFormat = "plain" | "markdown" | "tex";

export function guessClientDocumentFormat(documentPath: string): ClientDocumentFormat {
  const ext = fileExtensionLower(documentPath ?? "");

  if (ext === "md" || ext === "markdown") return "markdown";
  if (ext === "tex" || ext === "latex") return "tex";
  return "plain";
}

export function renderInlineProtectedText(
  text: string,
  format: ClientDocumentFormat,
  keyPrefix = "protected"
): ReactNode {
  const segments = resolveProtectedSegments(text, format);
  if (!segments || (segments.length === 1 && segments[0].kind === "text")) return text;

  return segments.map((segment, index) => {
    if (segment.kind === "text") return segment.text;
    return (
      <span
        key={`${keyPrefix}-${index}-${segment.text.length}`}
        className="inline-protected"
        data-protect-kind={segment.protectKind}
        title={`保护区：${segment.label}，AI 不会修改`}
      >
        {segment.text}
      </span>
    );
  });
}

function resolveProtectedSegments(
  text: string,
  format: ClientDocumentFormat
): ProtectedSegment[] | null {
  if (format === "markdown") {
    const likelyHasProtected =
      text.includes("`") ||
      text.includes("$") ||
      text.includes("[") ||
      text.includes("!") ||
      text.includes("<") ||
      text.includes("http://") ||
      text.includes("https://") ||
      text.includes("www.");
    return likelyHasProtected ? splitMarkdownInlineProtected(text) : null;
  }

  if (format === "tex") {
    const likelyHasProtected = text.includes("\\") || text.includes("$") || text.includes("%");
    return likelyHasProtected ? splitTexInlineProtected(text) : null;
  }

  return null;
}
