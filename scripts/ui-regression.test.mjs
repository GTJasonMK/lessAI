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

const part02 = read("src/styles/part-02.css");
const part04 = read("src/styles/part-04.css");

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

console.log("[ui-regression] OK");

