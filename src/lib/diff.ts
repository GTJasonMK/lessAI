import type { DiffSpan } from "./types";
import { normalizeNewlines } from "./helpers";

type DiffOp =
  | { type: "equal"; value: string }
  | { type: "insert"; value: string }
  | { type: "delete"; value: string };

const MAX_REFINED_CHARS = 8_000;
const DEFAULT_CONTEXT_CHARS = 80;
const DEFAULT_MAX_HUNKS = 120;

export interface DiffHunk {
  id: string;
  sequence: number;
  diffSpans: DiffSpan[];
  beforeText: string;
  afterText: string;
  insertedChars: number;
  deletedChars: number;
}

function splitLinesPreserveNewline(text: string): string[] {
  if (text.length === 0) return [""];
  const lines = text.split("\n");
  return lines.map((line, index) => (index < lines.length - 1 ? `${line}\n` : line));
}

function splitChars(text: string): string[] {
  return Array.from(text);
}

function countNonWhitespaceChars(text: string) {
  return text.replace(/\s+/g, "").length;
}

function myersDiff(before: ReadonlyArray<string>, after: ReadonlyArray<string>) {
  const n = before.length;
  const m = after.length;
  const max = n + m;
  const offset = max;
  const v = new Int32Array(2 * max + 1);

  // 使用 -1 作为哨兵值，便于比较。
  v.fill(-1);
  v[offset + 1] = 0;

  const trace: Int32Array[] = [];
  let finished = false;

  for (let d = 0; d <= max; d++) {
    for (let k = -d; k <= d; k += 2) {
      const kIndex = offset + k;

      let x: number;
      if (
        k === -d ||
        (k !== d && v[offset + k - 1] < v[offset + k + 1])
      ) {
        // 向下走：插入 after[y]
        x = v[offset + k + 1];
      } else {
        // 向右走：删除 before[x]
        x = v[offset + k - 1] + 1;
      }

      let y = x - k;
      while (x < n && y < m && before[x] === after[y]) {
        x += 1;
        y += 1;
      }

      v[kIndex] = x;

      if (x >= n && y >= m) {
        finished = true;
        break;
      }
    }

    trace.push(new Int32Array(v));
    if (finished) break;
  }

  // 回溯生成操作序列（forward order）
  let x = n;
  let y = m;
  const ops: DiffOp[] = [];

  for (let d = trace.length - 1; d > 0; d--) {
    const vPrev = trace[d - 1];
    const k = x - y;

    let prevK: number;
    if (k === -d || (k !== d && vPrev[offset + k - 1] < vPrev[offset + k + 1])) {
      prevK = k + 1;
    } else {
      prevK = k - 1;
    }

    const prevX = vPrev[offset + prevK];
    const prevY = prevX - prevK;

    while (x > prevX && y > prevY) {
      ops.push({ type: "equal", value: before[x - 1] });
      x -= 1;
      y -= 1;
    }

    if (x === prevX) {
      ops.push({ type: "insert", value: after[prevY] });
      y -= 1;
    } else {
      ops.push({ type: "delete", value: before[prevX] });
      x -= 1;
    }

    x = prevX;
    y = prevY;
  }

  while (x > 0 && y > 0) {
    ops.push({ type: "equal", value: before[x - 1] });
    x -= 1;
    y -= 1;
  }

  while (x > 0) {
    ops.push({ type: "delete", value: before[x - 1] });
    x -= 1;
  }

  while (y > 0) {
    ops.push({ type: "insert", value: after[y - 1] });
    y -= 1;
  }

  return ops.reverse();
}

function pushSpan(spans: DiffSpan[], type: DiffSpan["type"], text: string) {
  if (!text) return;
  const last = spans[spans.length - 1];
  if (last && last.type === type) {
    last.text += text;
    return;
  }
  spans.push({ type, text });
}

function diffTextByChars(beforeText: string, afterText: string): DiffSpan[] {
  const ops = myersDiff(splitChars(beforeText), splitChars(afterText));
  const spans: DiffSpan[] = [];

  for (const op of ops) {
    const type: DiffSpan["type"] =
      op.type === "equal" ? "unchanged" : op.type === "insert" ? "insert" : "delete";
    pushSpan(spans, type, op.value);
  }

  return spans;
}

export function diffTextByLines(beforeText: string, afterText: string): DiffSpan[] {
  const normalizedBefore = normalizeNewlines(beforeText);
  const normalizedAfter = normalizeNewlines(afterText);

  if (normalizedBefore === normalizedAfter) {
    return [{ type: "unchanged", text: normalizedAfter }];
  }

  const before = splitLinesPreserveNewline(normalizedBefore);
  const after = splitLinesPreserveNewline(normalizedAfter);
  const ops = myersDiff(before, after);

  const spans: DiffSpan[] = [];
  let pendingDeletes: string[] = [];
  let pendingInserts: string[] = [];

  const flushPending = () => {
    if (pendingDeletes.length === 0 && pendingInserts.length === 0) return;

    const deletedText = pendingDeletes.join("");
    const insertedText = pendingInserts.join("");
    pendingDeletes = [];
    pendingInserts = [];

    if (deletedText && insertedText) {
      const refined =
        deletedText.length + insertedText.length <= MAX_REFINED_CHARS
          ? diffTextByChars(deletedText, insertedText)
          : null;

      if (refined) {
        for (const span of refined) {
          pushSpan(spans, span.type, span.text);
        }
        return;
      }

      pushSpan(spans, "delete", deletedText);
      pushSpan(spans, "insert", insertedText);
      return;
    }

    if (deletedText) pushSpan(spans, "delete", deletedText);
    if (insertedText) pushSpan(spans, "insert", insertedText);
  };

  for (const op of ops) {
    if (op.type === "equal") {
      flushPending();
      pushSpan(spans, "unchanged", op.value);
      continue;
    }
    if (op.type === "delete") {
      pendingDeletes.push(op.value);
      continue;
    }
    pendingInserts.push(op.value);
  }

  flushPending();
  return spans;
}

function tailContext(text: string, maxChars: number) {
  if (!text) return "";
  const lastNewline = text.lastIndexOf("\n");
  const candidate = lastNewline >= 0 ? text.slice(lastNewline + 1) : text;
  if (candidate.length <= maxChars) return candidate;
  return candidate.slice(candidate.length - maxChars);
}

function splitHeadContext(text: string, maxChars: number) {
  if (!text) return { head: "", rest: "" };
  const newlineIndex = text.indexOf("\n");
  if (newlineIndex >= 0 && newlineIndex + 1 <= maxChars) {
    return { head: text.slice(0, newlineIndex + 1), rest: text.slice(newlineIndex + 1) };
  }
  if (text.length <= maxChars) return { head: text, rest: "" };
  return { head: text.slice(0, maxChars), rest: text.slice(maxChars) };
}

export function buildDiffHunks(
  spans: ReadonlyArray<DiffSpan>,
  options?: { contextChars?: number; maxHunks?: number }
): DiffHunk[] {
  const contextChars = Math.max(0, options?.contextChars ?? DEFAULT_CONTEXT_CHARS);
  const bridgeChars = contextChars * 2;
  const maxHunks = Math.max(1, options?.maxHunks ?? DEFAULT_MAX_HUNKS);
  const work = spans.map((item) => ({ ...item }));

  const hunks: DiffHunk[] = [];
  let lastUnchangedText = "";
  let i = 0;

  while (i < work.length) {
    const span = work[i];
    if (span.type === "unchanged") {
      lastUnchangedText = span.text;
      i += 1;
      continue;
    }

    const prefix = tailContext(lastUnchangedText, contextChars);
    const hunkSpans: DiffSpan[] = [];
    if (prefix) {
      pushSpan(hunkSpans, "unchanged", prefix);
    }

    let sawChange = false;
    while (i < work.length) {
      const current = work[i];

      if (current.type !== "unchanged") {
        sawChange = true;
        pushSpan(hunkSpans, current.type, current.text);
        i += 1;
        continue;
      }

      if (!sawChange) {
        lastUnchangedText = current.text;
        i += 1;
        continue;
      }

      // 变更对合并策略：
      // - 若两段变更之间的 unchanged 很短，则视为同一组（避免“随手改一下就几十个变更对”）
      // - 若 unchanged 很长，则切分为新的变更对，仅保留末尾/开头少量上下文
      let nextChangeIndex = i + 1;
      while (nextChangeIndex < work.length && work[nextChangeIndex].type === "unchanged") {
        nextChangeIndex += 1;
      }
      const hasNextChange = nextChangeIndex < work.length;

      if (!hasNextChange) {
        const { head } = splitHeadContext(current.text, contextChars);
        if (head) {
          pushSpan(hunkSpans, "unchanged", head);
        }
        i = nextChangeIndex;
        break;
      }

      if (current.text.length <= bridgeChars) {
        pushSpan(hunkSpans, "unchanged", current.text);
        i += 1;
        continue;
      }

      const { head, rest } = splitHeadContext(current.text, contextChars);
      if (head) {
        pushSpan(hunkSpans, "unchanged", head);
      }
      if (rest) {
        work[i] = { type: "unchanged", text: rest };
      } else {
        i += 1;
      }
      break;
    }

    let beforeText = "";
    let afterText = "";
    let insertedChars = 0;
    let deletedChars = 0;

    for (const item of hunkSpans) {
      if (item.type !== "insert") {
        beforeText += item.text;
      }
      if (item.type !== "delete") {
        afterText += item.text;
      }
      if (item.type === "insert") {
        insertedChars += countNonWhitespaceChars(item.text);
      }
      if (item.type === "delete") {
        deletedChars += countNonWhitespaceChars(item.text);
      }
    }

    const sequence = hunks.length + 1;
    hunks.push({
      id: `hunk-${sequence}`,
      sequence,
      diffSpans: hunkSpans,
      beforeText,
      afterText,
      insertedChars,
      deletedChars
    });

    if (hunks.length >= maxHunks) {
      break;
    }

    lastUnchangedText = "";
  }

  return hunks;
}
