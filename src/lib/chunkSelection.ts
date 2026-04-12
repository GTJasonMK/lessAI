import { buildChunkGroups } from "./chunkGroups";
import type { ChunkPreset, ChunkTask } from "./types";

function buildSelectedSet(selectedChunkIndices: readonly number[]) {
  return new Set(selectedChunkIndices);
}

function sortIndices(indices: Set<number>) {
  return Array.from(indices).sort((left, right) => left - right);
}

export function toggleSelectedChunkIndices(
  selectedChunkIndices: readonly number[],
  targetChunkIndices: readonly number[]
) {
  const selected = buildSelectedSet(selectedChunkIndices);
  const allSelected =
    targetChunkIndices.length > 0 &&
    targetChunkIndices.every((index) => selected.has(index));

  for (const index of targetChunkIndices) {
    if (allSelected) {
      selected.delete(index);
      continue;
    }
    selected.add(index);
  }

  return sortIndices(selected);
}

export function normalizeSelectedChunkIndices(
  chunks: readonly ChunkTask[],
  selectedChunkIndices: readonly number[],
  chunkPreset?: ChunkPreset | null
) {
  const allowed = new Set(
    chunks.filter((chunk) => !chunk.skipRewrite).map((chunk) => chunk.index)
  );

  const rawSelected = new Set<number>();
  for (const index of selectedChunkIndices) {
    if (!allowed.has(index)) continue;
    rawSelected.add(index);
  }

  const normalized = new Set<number>();
  for (const group of buildChunkGroups(chunks, chunkPreset)) {
    if (!group.editableIndices.some((index) => rawSelected.has(index))) {
      continue;
    }
    for (const index of group.editableIndices) {
      normalized.add(index);
    }
  }

  return sortIndices(normalized);
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

export function resolveSelectionTargetChunkIndices(
  chunks: readonly ChunkTask[],
  index: number,
  chunkPreset?: ChunkPreset | null
) {
  const group = buildChunkGroups(chunks, chunkPreset).find((candidate) =>
    candidate.chunkIndices.includes(index)
  );
  if (!group) return [];
  return group.editableIndices;
}

export function countSelectedChunkUnits(
  chunks: readonly ChunkTask[],
  selectedChunkIndices: readonly number[],
  chunkPreset?: ChunkPreset | null
) {
  if (!hasSelectedChunks(selectedChunkIndices)) {
    return 0;
  }

  const selected = buildSelectedSet(selectedChunkIndices);
  return buildChunkGroups(chunks, chunkPreset).filter((group) =>
    group.editableIndices.some((index) => selected.has(index))
  ).length;
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
