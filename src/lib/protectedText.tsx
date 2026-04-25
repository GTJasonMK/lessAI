import type { ReactNode } from "react";

import { splitMarkdownInlineProtected } from "./markdownProtectedSegments";
import { fileExtensionLower } from "./path";
import {
  DOCX_PLACEHOLDER_LABELS,
  PDF_PLACEHOLDER_LABELS
} from "./protectedTextPlaceholderLabels.generated";
import { type ProtectedSegment } from "./protectedTextShared";
import { splitTexInlineProtected } from "./texProtectedSegments";
import type { WritebackSlot } from "./types";

export type ClientDocumentFormat = "plain" | "markdown" | "tex" | "docx" | "pdf";

export function guessClientDocumentFormat(documentPath: string): ClientDocumentFormat {
  const ext = fileExtensionLower(documentPath ?? "");

  if (ext === "md" || ext === "markdown") return "markdown";
  if (ext === "tex" || ext === "latex") return "tex";
  if (ext === "docx") return "docx";
  if (ext === "pdf") return "pdf";
  return "plain";
}

export function renderInlineProtectedText(
  text: string,
  format: ClientDocumentFormat,
  keyPrefix = "protected",
  options?: { slot?: WritebackSlot | null }
): ReactNode {
  const segments = resolveProtectedSegments(text, format, options);
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
  format: ClientDocumentFormat,
  options?: { slot?: WritebackSlot | null }
): ProtectedSegment[] | null {
  const slotProtectKind = options?.slot?.presentation?.protectKind?.trim();
  if (slotProtectKind) {
    return [
      {
        kind: "protected",
        text,
        label: protectLabelForKind(slotProtectKind, format),
        protectKind: slotProtectKind
      }
    ];
  }

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

  if (format === "docx" || format === "pdf") {
    // When slot context exists and the slot is editable, avoid fallback placeholder guessing.
    if (options?.slot) return null;
    return splitFormatPlaceholders(text, format);
  }

  return null;
}

type PlaceholderFormat = Extract<ClientDocumentFormat, "docx" | "pdf">;

const DOCX_PLACEHOLDER_PATTERN = buildBracketPlaceholderPattern(DOCX_PLACEHOLDER_LABELS);
const PDF_PLACEHOLDER_PATTERN = buildBracketPlaceholderPattern(PDF_PLACEHOLDER_LABELS);
const PLACEHOLDER_RULES: Record<
  PlaceholderFormat,
  {
    pattern: RegExp;
    label: string;
    protectKind: string;
  }
> = {
  docx: {
    pattern: DOCX_PLACEHOLDER_PATTERN,
    label: "DOCX 占位符",
    protectKind: "docx-placeholder"
  },
  pdf: {
    pattern: PDF_PLACEHOLDER_PATTERN,
    label: "PDF 占位符",
    protectKind: "pdf-placeholder"
  }
};

function protectLabelForKind(
  protectKind: string,
  format: ClientDocumentFormat
): string {
  if (protectKind.startsWith("docx-")) return "DOCX 保护片段";
  if (protectKind.startsWith("pdf-")) return "PDF 保护片段";
  if (format === "docx") return "DOCX 保护片段";
  if (format === "pdf") return "PDF 保护片段";
  return "保护片段";
}

function buildBracketPlaceholderPattern(labels: readonly string[]): RegExp {
  return new RegExp(`\\[(?:${labels.map(escapeRegExpLiteral).join("|")})\\]`, "g");
}

function escapeRegExpLiteral(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function splitFormatPlaceholders(
  text: string,
  format: PlaceholderFormat
): ProtectedSegment[] | null {
  const rule = PLACEHOLDER_RULES[format];
  return splitBracketPlaceholders(text, rule.pattern, rule.label, rule.protectKind);
}

function splitBracketPlaceholders(
  text: string,
  pattern: RegExp,
  label: string,
  protectKind: string
): ProtectedSegment[] | null {
  if (!text.includes("[")) return null;

  const segments: ProtectedSegment[] = [];
  let cursor = 0;

  for (const match of text.matchAll(pattern)) {
    const full = match[0];
    const start = match.index;
    if (start == null) continue;
    if (start > cursor) {
      segments.push({ kind: "text", text: text.slice(cursor, start) });
    }
    segments.push({
      kind: "protected",
      text: full,
      label,
      protectKind
    });
    cursor = start + full.length;
  }

  if (segments.length === 0) return null;
  if (cursor < text.length) {
    segments.push({ kind: "text", text: text.slice(cursor) });
  }
  return segments;
}
