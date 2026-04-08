import { readFileSync } from "node:fs";
import assert from "node:assert/strict";

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

const part02 = read("src/styles/part-02.css");
const part04 = read("src/styles/part-04.css");
const documentActionBar = read("src/stages/workbench/document/DocumentActionBar.tsx");
const documentPanel = read("src/stages/workbench/DocumentPanel.tsx");
const documentFlow = read("src/stages/workbench/document/DocumentFlow.tsx");

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

console.log("[ui-regression] OK");
