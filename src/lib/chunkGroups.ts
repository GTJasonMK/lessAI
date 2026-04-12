import type { ChunkPreset, ChunkTask } from "./types";

const PARAGRAPH_SEPARATOR = "\n\n";
const SENTENCE_BOUNDARIES = new Set(["。", "！", "？", "；", "!", "?", ";", "."]);
const CLAUSE_BOUNDARIES = new Set([...SENTENCE_BOUNDARIES, "，", ","]);
const CLOSING_PUNCTUATION = new Set([
  '"',
  "'",
  "”",
  "’",
  "）",
  ")",
  "】",
  "]",
  "}",
  "」",
  "』",
  "》",
  "〉"
]);

export interface ChunkGroup {
  id: string;
  chunks: ChunkTask[];
  chunkIndices: number[];
  editableIndices: number[];
}

function createChunkGroup(chunks: ChunkTask[]) {
  const first = chunks[0];
  return {
    id: `group-${first?.index ?? 0}`,
    chunks,
    chunkIndices: chunks.map((chunk) => chunk.index),
    editableIndices: chunks.filter((chunk) => !chunk.skipRewrite).map((chunk) => chunk.index)
  } satisfies ChunkGroup;
}

function endsParagraph(chunk: ChunkTask) {
  return chunk.separatorAfter.includes(PARAGRAPH_SEPARATOR);
}

function buildGroupText(chunks: readonly ChunkTask[]) {
  return chunks.map((chunk) => `${chunk.sourceText}${chunk.separatorAfter}`).join("");
}

function lastNonWhitespaceIndex(text: string) {
  for (let index = text.length - 1; index >= 0; index -= 1) {
    if (!/\s/.test(text[index] ?? "")) {
      return index;
    }
  }
  return -1;
}

function boundarySetForPreset(chunkPreset?: ChunkPreset | null) {
  return chunkPreset === "clause" ? CLAUSE_BOUNDARIES : SENTENCE_BOUNDARIES;
}

function endsSemanticGroup(chunks: readonly ChunkTask[], chunkPreset?: ChunkPreset | null) {
  const text = buildGroupText(chunks);
  let index = lastNonWhitespaceIndex(text);
  if (index < 0) return false;

  while (index >= 0 && CLOSING_PUNCTUATION.has(text[index] ?? "")) {
    index -= 1;
  }
  if (index < 0) return false;

  return boundarySetForPreset(chunkPreset).has(text[index] ?? "");
}

export function buildChunkGroups(
  chunks: ReadonlyArray<ChunkTask>,
  chunkPreset?: ChunkPreset | null
) {
  if (chunkPreset == null) {
    return chunks.map((chunk) => createChunkGroup([chunk]));
  }

  const groups: ChunkGroup[] = [];
  let current: ChunkTask[] = [];

  for (const chunk of chunks) {
    current.push(chunk);
    const shouldClose =
      endsParagraph(chunk) ||
      (chunkPreset !== "paragraph" && endsSemanticGroup(current, chunkPreset));
    if (!shouldClose) {
      continue;
    }
    groups.push(createChunkGroup(current));
    current = [];
  }

  if (current.length > 0) {
    groups.push(createChunkGroup(current));
  }

  return groups;
}
