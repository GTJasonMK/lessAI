import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import assert from "node:assert/strict";
import ts from "typescript";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";

function read(path) {
  return readFileSync(new URL(`../${path}`, import.meta.url), "utf8");
}

function hasRule(css, selector, property, value) {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const prop = property.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const val = value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const re = new RegExp(`${escaped}\\s*\\{[\\s\\S]*?${prop}\\s*:\\s*${val}\\s*;`, "m");
  return re.test(css);
}

function assertRule(css, selector, property, value) {
  assert.ok(
    hasRule(css, selector, property, value),
    `期望 CSS 存在：${selector} { ${property}: ${value}; }`
  );
}

function assertNoRule(css, selector, property, value) {
  assert.ok(
    !hasRule(css, selector, property, value),
    `期望 CSS 不存在：${selector} { ${property}: ${value}; }`
  );
}

function assertIncludes(text, snippet) {
  assert.ok(text.includes(snippet), `期望内容包含：${snippet}`);
}

function assertNotIncludes(text, snippet) {
  assert.ok(!text.includes(snippet), `期望内容不包含：${snippet}`);
}

function assertMatches(text, pattern, message) {
  assert.ok(pattern.test(text), message);
}

async function loadProtectedTextModule() {
  const tempRoot = join(process.cwd(), ".tmp");
  mkdirSync(tempRoot, { recursive: true });
  const dir = mkdtempSync(join(tempRoot, "lessai-protected-text-"));
  const modules = [
    ["src/lib/protectedText.tsx", "protectedText.tsx"],
    ["src/lib/markdownProtectedSegments.ts", "markdownProtectedSegments.ts"],
    ["src/lib/protectedTextShared.ts", "protectedTextShared.ts"],
    ["src/lib/texProtectedSegments.ts", "texProtectedSegments.ts"]
  ];
  const file = join(dir, "protectedText.mjs");

  try {
    for (const [path, fileName] of modules) {
      const source = read(path);
      const transpiled = ts.transpileModule(source, {
        compilerOptions: {
          module: ts.ModuleKind.ES2022,
          target: ts.ScriptTarget.ES2022,
          jsx: ts.JsxEmit.ReactJSX
        },
        fileName
      }).outputText;
      const rewritten = transpiled.replace(/from\s+["'](\.\/[^"']+)["']/g, 'from "$1.mjs"');
      writeFileSync(join(dir, fileName.replace(/\.(ts|tsx)$/, ".mjs")), rewritten, "utf8");
    }
    return await import(pathToFileURL(file).href);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

async function loadChunkSelectionModule() {
  const tempRoot = join(process.cwd(), ".tmp");
  mkdirSync(tempRoot, { recursive: true });
  const dir = mkdtempSync(join(tempRoot, "lessai-chunk-selection-"));
  const file = join(dir, "chunkSelection.mjs");

  try {
    for (const [path, fileName] of [
      ["src/lib/chunkSelection.ts", "chunkSelection.ts"],
      ["src/lib/chunkGroups.ts", "chunkGroups.ts"]
    ]) {
      const source = read(path);
      const transpiled = ts.transpileModule(source, {
        compilerOptions: {
          module: ts.ModuleKind.ES2022,
          target: ts.ScriptTarget.ES2022
        },
        fileName
      }).outputText;
      const rewritten = transpiled.replace(/from\s+["'](\.\/[^"']+)["']/g, 'from "$1.mjs"');
      writeFileSync(join(dir, fileName.replace(/\.ts$/, ".mjs")), rewritten, "utf8");
    }
    const chunkSelection = await import(pathToFileURL(file).href);
    const chunkGroups = await import(pathToFileURL(join(dir, "chunkGroups.mjs")).href);
    return { ...chunkSelection, ...chunkGroups };
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

const part02 = read("src/styles/part-02.css");
const part04 = read("src/styles/part-04.css");
const documentActionBar = read("src/stages/workbench/document/DocumentActionBar.tsx");
const documentPanel = read("src/stages/workbench/DocumentPanel.tsx");
const documentFlow = read("src/stages/workbench/document/DocumentFlow.tsx");
const { renderInlineProtectedText } = await loadProtectedTextModule();
const { buildChunkGroups, normalizeSelectedChunkIndices } = await loadChunkSelectionModule();

const paragraphChunks = [
  {
    index: 0,
    sourceText: "【填写说明：重点介绍本作品的主题创意来源，产生背景，作品的用户群体、主要功能与特色、应用价值、推广前景等。",
    separatorAfter: "",
    skipRewrite: false,
    presentation: null,
    status: "idle",
    errorMessage: null
  },
  {
    index: 1,
    sourceText: "建议不超过1页",
    separatorAfter: "",
    skipRewrite: false,
    presentation: { bold: false, italic: false, underline: false, href: null, writebackKey: "r:red" },
    status: "idle",
    errorMessage: null
  },
  {
    index: 2,
    sourceText: "】",
    separatorAfter: "\n\n",
    skipRewrite: false,
    presentation: null,
    status: "idle",
    errorMessage: null
  },
  {
    index: 3,
    sourceText: "下一段正文。",
    separatorAfter: "",
    skipRewrite: false,
    presentation: null,
    status: "idle",
    errorMessage: null
  }
];

assert.deepEqual(
  normalizeSelectedChunkIndices(paragraphChunks, [1], "paragraph"),
  [0, 1, 2],
  "段落级模式下，选中段内任一可改写子片段时，应扩展为整段的可改写子片段"
);

assert.deepEqual(
  normalizeSelectedChunkIndices(paragraphChunks, [1, 3], "paragraph"),
  [0, 1, 2, 3],
  "段落级模式下，应按段落单元归一化多选范围"
);

const clauseChunks = [
  {
    index: 0,
    sourceText: "硬件部署",
    separatorAfter: "",
    skipRewrite: false,
    presentation: null,
    status: "idle",
    errorMessage: null
  },
  {
    index: 1,
    sourceText: "：",
    separatorAfter: "",
    skipRewrite: false,
    presentation: { bold: true, italic: false, underline: false, href: null, writebackKey: "r:bold" },
    status: "idle",
    errorMessage: null
  },
  {
    index: 2,
    sourceText: "认知节点部署于 Dell PowerEdge R750xa，",
    separatorAfter: "",
    skipRewrite: false,
    presentation: null,
    status: "idle",
    errorMessage: null
  },
  {
    index: 3,
    sourceText: "运行 Neo4j + ChromaDB；",
    separatorAfter: "",
    skipRewrite: false,
    presentation: null,
    status: "idle",
    errorMessage: null
  },
  {
    index: 4,
    sourceText: "验证节点部署于 Lenovo ThinkStation P3。",
    separatorAfter: "\n\n",
    skipRewrite: false,
    presentation: null,
    status: "idle",
    errorMessage: null
  }
];

assert.deepEqual(
  buildChunkGroups(clauseChunks, "clause").map((group) => group.chunkIndices),
  [
    [0, 1, 2],
    [3],
    [4]
  ],
  "小句模式下，应把同一语义小句内的样式碎块归并成一个可见单元"
);

assert.deepEqual(
  normalizeSelectedChunkIndices(clauseChunks, [1], "clause"),
  [0, 1, 2],
  "小句模式下，选中冒号等样式碎块时，应扩展为整句对应的小句单元"
);

const sentenceChunks = [
  {
    index: 0,
    sourceText: "前文 ",
    separatorAfter: "",
    skipRewrite: false,
    presentation: null,
    status: "idle",
    errorMessage: null
  },
  {
    index: 1,
    sourceText: "[公式]",
    separatorAfter: "",
    skipRewrite: true,
    presentation: null,
    status: "idle",
    errorMessage: null
  },
  {
    index: 2,
    sourceText: " 后文。",
    separatorAfter: "",
    skipRewrite: false,
    presentation: null,
    status: "idle",
    errorMessage: null
  },
  {
    index: 3,
    sourceText: "下一句。",
    separatorAfter: "",
    skipRewrite: false,
    presentation: null,
    status: "idle",
    errorMessage: null
  }
];

assert.deepEqual(
  buildChunkGroups(sentenceChunks, "sentence").map((group) => group.chunkIndices),
  [
    [0, 1, 2],
    [3]
  ],
  "整句模式下，应保留句内保护区，但把整句作为一个可见单元"
);

assert.deepEqual(
  normalizeSelectedChunkIndices(sentenceChunks, [2], "sentence"),
  [0, 2],
  "整句模式下，选中句内任一可编辑碎块时，应扩展为整句内全部可编辑碎块"
);

function renderTexMarkup(text) {
  return renderToStaticMarkup(
    React.createElement(
      React.Fragment,
      null,
      renderInlineProtectedText(text, "tex", "ui-regression")
    )
  );
}

// 1) 审阅区动作按钮不应依赖横向滚动（避免“左滑右滑”）
assertNoRule(
  part02,
  ".workbench-review-panel .workbench-review-actionbar-buttons",
  "overflow-x",
  "auto"
);

// 2) 审阅视图切换条不应依赖横向滚动（按钮应固定在一个位置）
assertNoRule(part04, ".review-switches", "overflow-x", "auto");

// 3) 文档面板 header 的 action 区域必须允许 shrink，避免按钮在尾部被裁切
assertRule(part02, ".workbench-doc-panel .panel-action", "flex", "0 1 auto");

// 4) “已选 N 段”不能继续占用顶部 action bar
assertNotIncludes(documentActionBar, "已选 {selectedChunkCount} 段");

// 5) “已选 N 段”不能塞进面板副标题
assertNotIncludes(documentPanel, "已选 ${selectedChunkIndices.length} 段");

// 6) “已选 N 段”应显示在内容区状态条
assertIncludes(documentFlow, "document-flow-status");

// 7) 选择状态条必须是内容区右上角浮层，不能参与正常文档流
assertRule(part04, ".document-flow-wrap", "position", "relative");
assertRule(part04, ".document-flow-status", "position", "absolute");
assertRule(part04, ".document-flow-status", "top", "0");
assertRule(part04, ".document-flow-status", "right", "0");

// 8) 文案切换时，“处理所选 / 开始批处理 / 暂停 / 继续”主按钮不能改变整排布局
assertRule(part02, ".workbench-doc-actionbar-right .toolbar-button.is-run-action", "inline-size", "152px");
assertRule(part02, ".workbench-doc-actionbar-right .toolbar-button.is-run-action", "min-width", "152px");

// 9) TeX 的 \\texttt{...} 应被标成单个保护区
const textttMarkup = renderTexMarkup(
  "还包含一段命令 token: \\texttt{cargo fmt --check}。"
);
assertMatches(
  textttMarkup,
  /<span[^>]*class="inline-protected"[^>]*>\\texttt\{cargo fmt --check\}<\/span>/,
  "期望 \\texttt{...} 被渲染为单个保护区"
);

// 10) TeX 的 \\href{...}{...} 应整体标成保护区
const hrefMarkup = renderTexMarkup(
  "这段里还有一个链接：\\href{https://example.com/docs}{https://example.com/docs}。"
);
assertMatches(
  hrefMarkup,
  /<span[^>]*class="inline-protected"[^>]*>\\href\{https:\/\/example\.com\/docs\}\{https:\/\/example\.com\/docs\}<\/span>/,
  "期望 \\href{...}{...} 被整体渲染为保护区"
);

// 11) 可改写文本命令不应把正文参数整段锁死，只高亮命令壳
const textbfMarkup = renderTexMarkup("这是 \\textbf{很重要} 的句子。");
assertMatches(
  textbfMarkup,
  /\\textbf\{<\/span>很重要<span[^>]*class="inline-protected"[^>]*>\}<\/span>/,
  "期望 \\textbf{...} 只高亮命令语法，不锁死正文参数"
);

console.log("[ui-regression] OK");
