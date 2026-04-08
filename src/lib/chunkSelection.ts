import type { ChunkTask } from "./types";

function buildSelectedSet(selectedChunkIndices: readonly number[]) {
  return new Set(selectedChunkIndices);
}

export function toggleSelectedChunkIndex(
  selectedChunkIndices: readonly number[],
  index: number
) {
  const selected = buildSelectedSet(selectedChunkIndices);
  if (selected.has(index)) {
    selected.delete(index);
  } else {
    selected.add(index);
  }
  return Array.from(selected).sort((left, right) => left - right);
}

export function normalizeSelectedChunkIndices(
  chunks: readonly ChunkTask[],
  selectedChunkIndices: readonly number[]
) {
  const allowed = new Set(
    chunks.filter((chunk) => !chunk.skipRewrite).map((chunk) => chunk.index)
  );
  const selected = new Set<number>();
  for (const index of selectedChunkIndices) {
    if (!allowed.has(index)) continue;
    selected.add(index);
  }
  return Array.from(selected).sort((left, right) => left - right);
}

export function hasSelectedChunks(selectedChunkIndices: readonly number[]) {
  return selectedChunkIndices.length > 0;
}

export function isChunkSelected(
  selectedChunkIndices: readonly number[],
  index: number
) {
  return selectedChunkIndices.includes(index);
}

function matchesTarget(
  selectedChunkIndices: readonly number[],
  chunk: ChunkTask
) {
  return (
    !chunk.skipRewrite &&
    (!hasSelectedChunks(selectedChunkIndices) ||
      selectedChunkIndices.includes(chunk.index))
  );
}

export function findNextManualTargetChunk(
  chunks: readonly ChunkTask[],
  selectedChunkIndices: readonly number[]
) {
  return (
    chunks.find(
      (chunk) =>
        matchesTarget(selectedChunkIndices, chunk) &&
        (chunk.status === "idle" || chunk.status === "failed")
    ) ?? null
  );
}

export function findAutoPendingTargetChunks(
  chunks: readonly ChunkTask[],
  selectedChunkIndices: readonly number[]
) {
  return chunks.filter(
    (chunk) =>
      matchesTarget(selectedChunkIndices, chunk) && chunk.status !== "done"
  );
}
