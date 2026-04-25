import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

export function read(path) {
  return readFileSync(new URL(`../${path}`, import.meta.url), "utf8");
}

export function assertIncludes(text, snippet, message) {
  assert.ok(text.includes(snippet), message ?? `期望内容包含：${snippet}`);
}

export function assertNotIncludes(text, snippet) {
  assert.ok(!text.includes(snippet), `期望内容不包含：${snippet}`);
}

export function assertMatches(text, pattern, message) {
  assert.ok(pattern.test(text), message);
}
