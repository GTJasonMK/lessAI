import assert from "node:assert/strict";

import { assertIncludes, read } from "./test-helpers.mjs";

const docxRustSource = read("src-tauri/src/adapters/docx/placeholders.rs");
const pdfRustSource = read("src-tauri/src/adapters/pdf.rs");
const protectedTextSource = read("src/lib/protectedText.tsx");
const generatedSource = read("src/lib/protectedTextPlaceholderLabels.generated.ts");

assertIncludes(
  protectedTextSource,
  'from "./protectedTextPlaceholderLabels.generated"',
  "前端 protectedText.tsx 应从生成文件读取占位符标签"
);

const docxBackendLabels = [
  ...docxRustSource.matchAll(/DOCX_[A-Z_]+_PLACEHOLDER:\s*&str\s*=\s*"\[([^\]"]+)\]";/g),
].map((match) => match[1]);
assert.ok(docxBackendLabels.length > 0, "后端 DOCX placeholders 未解析到占位符常量");

const pdfBackendLabels = [
  ...pdfRustSource.matchAll(/PDF_[A-Z_]+_PLACEHOLDER:\s*&str\s*=\s*"\[([^\]"]+)\]";/g),
].map((match) => match[1]);
assert.ok(pdfBackendLabels.length > 0, "后端 PDF placeholders 未解析到占位符常量");

function normalized(values) {
  return [...new Set(values)].sort();
}

function onlyIn(left, right) {
  const rightSet = new Set(right);
  return left.filter((value) => !rightSet.has(value));
}

function frontendLabelsFor(listName) {
  const match = generatedSource.match(
    new RegExp(`const ${listName} = \\[([\\s\\S]*?)\\] as const;`)
  );
  assert.ok(
    match,
    `前端生成文件 protectedTextPlaceholderLabels.generated.ts 缺少 ${listName} 列表。请先运行 node scripts/generate-placeholder-labels.mjs`
  );
  return [...match[1].matchAll(/"([^"]+)"/g)].map((item) => item[1]);
}

function assertPlaceholderLabelsSynced(domain, backendLabels, frontendLabels) {
  const backendNormalized = normalized(backendLabels);
  const frontendNormalized = normalized(frontendLabels);
  const onlyBackend = onlyIn(backendNormalized, frontendNormalized);
  const onlyFrontend = onlyIn(frontendNormalized, backendNormalized);

  assert.deepEqual(
    frontendNormalized,
    backendNormalized,
    `${domain} 占位符标签不一致。仅后端: ${onlyBackend.join(", ") || "(无)"}；仅前端: ${
      onlyFrontend.join(", ") || "(无)"
    }`
  );
}

assertPlaceholderLabelsSynced(
  "DOCX",
  docxBackendLabels,
  frontendLabelsFor("DOCX_PLACEHOLDER_LABELS")
);
assertPlaceholderLabelsSynced("PDF", pdfBackendLabels, frontendLabelsFor("PDF_PLACEHOLDER_LABELS"));

console.log("[placeholder-sync] OK");
