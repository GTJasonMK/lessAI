import type { ChunkTask, EditorChunkEdit } from "./types";

export type EditorChunkOverrides = Record<number, string>;

export function buildEditorTextFromChunks(
  chunks: ReadonlyArray<ChunkTask>,
  overrides: EditorChunkOverrides
) {
  return chunks
    .map((chunk) => `${resolveEditorChunkText(chunk, overrides)}${chunk.separatorAfter}`)
    .join("");
}

export function buildEditorChunkEdits(
  chunks: ReadonlyArray<ChunkTask>,
  overrides: EditorChunkOverrides
): EditorChunkEdit[] {
  return chunks
    .filter((chunk) => !chunk.skipRewrite)
    .map((chunk) => ({
      index: chunk.index,
      text: resolveEditorChunkText(chunk, overrides)
    }));
}

export function resolveEditorChunkText(
  chunk: ChunkTask,
  overrides: EditorChunkOverrides
) {
  return overrides[chunk.index] ?? chunk.sourceText;
}

export function applyEditorChunkOverride(
  overrides: EditorChunkOverrides,
  chunk: ChunkTask,
  nextText: string
) {
  if (chunk.skipRewrite) return overrides;
  const normalized = nextText;
  if (normalized === chunk.sourceText) {
    const { [chunk.index]: _removed, ...rest } = overrides;
    return rest;
  }
  return {
    ...overrides,
    [chunk.index]: normalized
  };
}
