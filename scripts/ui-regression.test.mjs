import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import assert from "node:assert/strict";
import ts from "typescript";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import {
  assertIncludes,
  assertMatches,
  assertNotIncludes,
  read
} from "./test-helpers.mjs";

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

function rewriteRelativeImports(code) {
  return code.replace(/from\s+["']((?:\.\.?\/)[^"']+)["']/g, 'from "$1.mjs"');
}

async function loadProtectedTextModule() {
  const tempRoot = join(process.cwd(), ".tmp");
  mkdirSync(tempRoot, { recursive: true });
  const dir = mkdtempSync(join(tempRoot, "lessai-protected-text-"));
  const modules = [
    ["src/lib/protectedText.tsx", "protectedText.tsx"],
    [
      "src/lib/protectedTextPlaceholderLabels.generated.ts",
      "protectedTextPlaceholderLabels.generated.ts"
    ],
    ["src/lib/markdownProtectedSegments.ts", "markdownProtectedSegments.ts"],
    ["src/lib/path.ts", "path.ts"],
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
      const rewritten = rewriteRelativeImports(transpiled);
      writeFileSync(join(dir, fileName.replace(/\.(ts|tsx)$/, ".mjs")), rewritten, "utf8");
    }
    return await import(pathToFileURL(file).href);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

async function loadReviewSuggestionRowModel() {
  const tempRoot = join(process.cwd(), ".tmp");
  mkdirSync(tempRoot, { recursive: true });
  const dir = mkdtempSync(join(tempRoot, "lessai-review-row-model-"));

  try {
    const source = read("src/stages/workbench/review/reviewSuggestionRowModel.ts");
    const transpiled = ts.transpileModule(source, {
      compilerOptions: {
        module: ts.ModuleKind.ES2022,
        target: ts.ScriptTarget.ES2022
      },
      fileName: "reviewSuggestionRowModel.ts"
    }).outputText;
    const rewritten = rewriteRelativeImports(transpiled);
    writeFileSync(join(dir, "reviewSuggestionRowModel.mjs"), rewritten, "utf8");

    return await import(pathToFileURL(join(dir, "reviewSuggestionRowModel.mjs")).href);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

async function loadDocumentFlowNavigationModule() {
  const tempRoot = join(process.cwd(), ".tmp");
  mkdirSync(tempRoot, { recursive: true });
  const dir = mkdtempSync(join(tempRoot, "lessai-document-flow-navigation-"));

  try {
    const source = read("src/stages/workbench/document/documentFlowNavigation.ts");
    const transpiled = ts.transpileModule(source, {
      compilerOptions: {
        module: ts.ModuleKind.ES2022,
        target: ts.ScriptTarget.ES2022
      },
      fileName: "documentFlowNavigation.ts"
    }).outputText;
    const rewritten = rewriteRelativeImports(transpiled);
    writeFileSync(join(dir, "documentFlowNavigation.mjs"), rewritten, "utf8");

    return await import(pathToFileURL(join(dir, "documentFlowNavigation.mjs")).href);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

async function loadHelpersModule() {
  const tempRoot = join(process.cwd(), ".tmp");
  mkdirSync(tempRoot, { recursive: true });
  const dir = mkdtempSync(join(tempRoot, "lessai-helpers-"));
  const modules = [
    ["src/lib/helpers.ts", "helpers.ts"],
    ["src/lib/documentCapabilities.ts", "documentCapabilities.ts"],
    ["src/lib/path.ts", "path.ts"]
  ];

  try {
    for (const [path, fileName] of modules) {
      const source = read(path);
      const transpiled = ts.transpileModule(source, {
        compilerOptions: {
          module: ts.ModuleKind.ES2022,
          target: ts.ScriptTarget.ES2022
        },
        fileName
      }).outputText;
      const rewritten = rewriteRelativeImports(transpiled);
      writeFileSync(join(dir, fileName.replace(/\.ts$/, ".mjs")), rewritten, "utf8");
    }

    return await import(pathToFileURL(join(dir, "helpers.mjs")).href);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

const part02 = read("src/styles/part-02.css");
const part03 = read("src/styles/part-03.css");
const part04 = read("src/styles/part-04.css");
const documentActionBar = read("src/stages/workbench/document/DocumentActionBar.tsx");
const documentPanel = read("src/stages/workbench/DocumentPanel.tsx");
const documentFlow = read("src/stages/workbench/document/DocumentFlow.tsx");
const paragraphDocumentFlow = read("src/stages/workbench/document/ParagraphDocumentFlow.tsx");
const structuredSlotEditor = read("src/stages/workbench/document/StructuredSlotEditor.tsx");
const workspaceBar = read("src/app/components/WorkspaceBar.tsx");
const settingsTypes = read("src/lib/types.ts");
const settingsConstants = read("src/lib/constants.ts");
const rewriteStrategyPage = read("src/components/settings/RewriteStrategyPage.tsx");
const settingsHandlers = read("src/app/hooks/useSettingsHandlers.ts");
const documentActions = read("src/app/hooks/useDocumentActions.ts");
const documentFinalizeActions = read("src/app/hooks/useDocumentFinalizeActions.ts");
const documentScrollRestore = read("src/app/hooks/useDocumentScrollRestore.ts");
const appSource = read("src/App.tsx");
const rewriteUnitSelection = read("src/lib/rewriteUnitSelection.ts");
const workbenchStage = read("src/stages/WorkbenchStage.tsx");
const reviewPanel = read("src/stages/workbench/ReviewPanel.tsx");
const reviewActionBar = read("src/stages/workbench/review/ReviewActionBar.tsx");
const reviewEmptyState = read("src/stages/workbench/review/ReviewEmptyState.tsx");
const suggestionReviewPane = read("src/stages/workbench/review/SuggestionReviewPane.tsx");
const reviewSuggestionRow = read("src/stages/workbench/review/ReviewSuggestionRow.tsx");
const useRewriteActions = read("src/app/hooks/useRewriteActions.ts");
const useSuggestionActions = read("src/app/hooks/useSuggestionActions.ts");
const { renderInlineProtectedText } = await loadProtectedTextModule();
const {
  buildSuggestionRowActionState,
  buildSuggestionRowPrimaryActionLabel,
  buildSuggestionRowTitle
} = await loadReviewSuggestionRowModel();
const { shouldScrollToActiveRewriteUnit } = await loadDocumentFlowNavigationModule();
const { getSessionStats, summarizeRewriteUnitSuggestions } = await loadHelpersModule();

assertIncludes(workspaceBar, 'className="workspace-bar-status-row"');
assertIncludes(workspaceBar, 'className="workspace-bar-path-line"');
assertIncludes(workspaceBar, 'className="workspace-bar-path-text"');
assertIncludes(appSource, 'from "./lib/windowDrag"');
assertIncludes(workspaceBar, 'from "../../lib/windowDrag"');
assertIncludes(appSource, "isWindowDragExcludedTarget(event.target)");
assertIncludes(workspaceBar, "isWindowDragExcludedTarget(event.target)");
assertNotIncludes(appSource, "const WINDOW_DRAG_EXCLUDED_SELECTOR = [");
assertNotIncludes(workspaceBar, "const HEADER_DRAG_EXCLUDED_SELECTOR = [");
assertIncludes(settingsTypes, "unitsPerBatch: number;");
assertIncludes(settingsConstants, "unitsPerBatch: 1");
assertIncludes(rewriteStrategyPage, "单批处理单元数");
assertIncludes(rewriteStrategyPage, 'onUpdateNumberSetting("unitsPerBatch", event.target.value)');
assertIncludes(settingsHandlers, '"unitsPerBatch"');
assertNotIncludes(workspaceBar, 'className="workspace-bar-session"');
assertNotIncludes(workspaceBar, "title={rawTitle}");
assertNotIncludes(workspaceBar, 'className="workspace-bar-session-text"');
assertNotIncludes(workspaceBar, "workspace-bar-path-chip");
assertNotIncludes(workspaceBar, "formatTopbarTitle");
assertNotIncludes(workspaceBar, "formatTopbarPath");
assertRule(part02, ".workspace-bar-status-row", "display", "flex");
assertRule(part02, ".workspace-bar-path-line", "display", "flex");
assertRule(part02, ".workspace-bar-path-text", "text-overflow", "ellipsis");
assertRule(part03, ".status-badge", "white-space", "nowrap");
assertRule(
  part04,
  ".structured-editor-slot.is-editable.is-underline:focus",
  "text-decoration",
  "none"
);
assertRule(
  part04,
  ".structured-editor-slot.is-editable.is-link:focus",
  "text-decoration",
  "none"
);
assertRule(part04, ".review-suggestion-row-mainline .status-badge", "flex", "0 0 auto");
assertNotIncludes(
  paragraphDocumentFlow,
  "[activeChunkIndex, groups, sessionId]",
  "写回刷新 session 时，不应因为 groups/sessionId 变化再次自动滚动到激活块"
);
assertNotIncludes(
  structuredSlotEditor,
  "chunkNodesRef.current[firstEditable.index]?.focus();\n    }, [session.chunks]);",
  "结构化编辑器写回后不应因为旧 chunks 语义再次聚焦首个可编辑块"
);
assertIncludes(
  structuredSlotEditor,
  "session.rewriteUnits.map((rewriteUnit) => {",
  "结构化编辑页应与主页面一致，按 rewrite unit 作为展示分组骨架"
);
assertNotIncludes(
  structuredSlotEditor,
  "session.writebackSlots.map((slot) => {",
  "结构化编辑页不应再按 writeback slot 平铺渲染，避免与主页面分块不一致"
);
assertNotIncludes(
  documentActions,
  "applySessionState(updated, selectDefaultChunkIndex(updated));",
  "编辑保存后应保留当前激活块，而不是重置到默认块"
);
assertIncludes(documentScrollRestore, "export function useDocumentScrollRestore()");
assertIncludes(documentScrollRestore, "const documentScrollRef = useRef<HTMLDivElement | null>(null);");
assertIncludes(
  documentScrollRestore,
  "const pendingRestoreRef = useRef<ScrollRestoreProgress | null>(null);"
);
assertIncludes(documentScrollRestore, "node.scrollTop = pending.targetScrollTop;");
assertIncludes(documentPanel, "documentScrollRef: MutableRefObject<HTMLDivElement | null>;");
assertIncludes(documentPanel, '<div ref={documentScrollRef} className="paper-content scroll-region">');
assertIncludes(documentActions, "captureDocumentScrollPosition: () => number | null;");
assertIncludes(documentActions, "const preservedScrollTop = captureDocumentScrollPosition();");
assertIncludes(documentActions, "runSessionActionWithScroll({");
assertIncludes(documentFinalizeActions, "captureDocumentScrollPosition: () => number | null;");
assertIncludes(documentFinalizeActions, "const preservedScrollTop = captureDocumentScrollPosition();");
assertIncludes(documentFinalizeActions, "restoreLoadedSessionWithScroll({");
assertIncludes(appSource, 'import { useDocumentScrollRestore } from "./app/hooks/useDocumentScrollRestore";');
assertIncludes(appSource, "const { documentScrollRef, captureDocumentScrollPosition, restoreDocumentScrollPosition } =");
assertIncludes(reviewPanel, 'title="建议"');
assertIncludes(reviewPanel, 'subtitle="建议列表"');
assertNotIncludes(reviewPanel, 'title="审阅"');
assertIncludes(suggestionReviewPane, 'className="review-summary-strip"');
assertNotIncludes(suggestionReviewPane, '当前 #{');
assertIncludes(suggestionReviewPane, "待处理：{currentStats.unitsProposed}");
assertNotIncludes(suggestionReviewPane, "待审阅：{currentStats.unitsProposed}");
assertNotIncludes(suggestionReviewPane, "待审阅：{currentStats.suggestionsProposed}");
assertIncludes(suggestionReviewPane, "<ReviewSuggestionRow");
assertNotIncludes(reviewSuggestionRow, "StatusBadge");
assertIncludes(reviewSuggestionRow, "buildSuggestionRowPrimaryActionLabel(suggestion.decision)");
assertIncludes(reviewSuggestionRow, '`is-${suggestion.decision}`');
assertIncludes(reviewSuggestionRow, 'className="review-suggestion-row-state-dot"');
assertIncludes(reviewSuggestionRow, '<span>删除</span>');
assertIncludes(reviewSuggestionRow, '<span>···</span>');
assertRule(part04, ".review-suggestion-row.is-proposed", "border-color", "rgba(239, 193, 34, 0.28)");
assertRule(part04, ".review-suggestion-row.is-applied", "border-color", "rgba(31, 122, 60, 0.24)");
assertRule(part04, ".review-suggestion-row.is-dismissed", "border-color", "rgba(20, 20, 20, 0.12)");
assertRule(part04, ".review-suggestion-row-state-dot", "width", "8px");
assertNotIncludes(suggestionReviewPane, 'className="diff-view"');
assertNotIncludes(workbenchStage, "reviewView");
assertNotIncludes(reviewPanel, "reviewView");
assertNotIncludes(reviewActionBar, "reviewView");
assertNotIncludes(appSource, "reviewView");
assertNotIncludes(useRewriteActions, "setReviewView");
assertNotIncludes(useSuggestionActions, "setReviewView");
assertNotIncludes(useRewriteActions, "修改对");
assertNotIncludes(reviewEmptyState, 'label="打开文件"');
assertNotIncludes(reviewEmptyState, "审阅区会展示");
assertIncludes(reviewEmptyState, "这里会展示建议与候选稿");
assertIncludes(rewriteUnitSelection, "normalizeSelectedRewriteUnitIds");
assertIncludes(rewriteUnitSelection, "resolveOptimisticManualRunningRewriteUnitId");
assertIncludes(documentFinalizeActions, "stats.unitsProposed > 0");
assertIncludes(documentFinalizeActions, "仍有 ${stats.unitsProposed} 段待处理");
assertIncludes(documentFinalizeActions, "待处理：${stats.unitsProposed}（会一起删除）");
assertIncludes(documentFinalizeActions, "待处理：0");
assertNotIncludes(documentFinalizeActions, "stats.suggestionsProposed > 0");
assertNotIncludes(documentFinalizeActions, "待审阅");
assertNotIncludes(documentPanel, "右侧审阅");
assertNotIncludes(useRewriteActions, "请在右侧审阅");
assertNotIncludes(documentFlow, "审阅最小单元");
assertNotIncludes(settingsHandlers, "审阅");

const sampleSuggestion = {
  id: "sg-1",
  sequence: 12,
  rewriteUnitId: "unit-1",
  beforeText: "手工统计问卷结果",
  afterText: "自动汇总问卷结果，压缩后半句长度",
  diffSpans: [],
  decision: "applied",
  slotUpdates: [],
  createdAt: "2026-04-18T10:42:00.000Z",
  updatedAt: "2026-04-18T10:42:00.000Z"
};

assert.equal(
  buildSuggestionRowTitle(sampleSuggestion, 40),
  "#12 自动汇总问卷结果，压缩后半句长度"
);

assert.equal(buildSuggestionRowPrimaryActionLabel("proposed"), "应用");
assert.equal(buildSuggestionRowPrimaryActionLabel("applied"), "忽略");
assert.equal(buildSuggestionRowPrimaryActionLabel("dismissed"), "应用");
assertIncludes(
  reviewSuggestionRow,
  'const showMenu = actionState.retryVisible;'
);
assertIncludes(reviewSuggestionRow, 'suggestion.decision === "applied"');
assertIncludes(reviewSuggestionRow, "onClick: onApply");

assert.deepEqual(
  buildSuggestionRowActionState({
    suggestionId: "sg-1",
    decision: "applied",
    busyAction: null,
    anyBusy: false,
    editorMode: false,
    rewriteRunning: false,
    rewritePaused: false,
    settingsReady: true,
    rewriteUnitFailed: true
  }),
  {
    applyBusy: false,
    applyDisabled: true,
    deleteBusy: false,
    deleteDisabled: false,
    dismissBusy: false,
    dismissDisabled: false,
    retryBusy: false,
    retryDisabled: false,
    retryVisible: true
  }
);

const mixedUnitSuggestions = [
  {
    id: "sg-applied",
    sequence: 1,
    rewriteUnitId: "unit-1",
    beforeText: "原文一",
    afterText: "已应用版本",
    diffSpans: [],
    decision: "applied",
    slotUpdates: [],
    createdAt: "2026-04-18T10:40:00.000Z",
    updatedAt: "2026-04-18T10:40:00.000Z"
  },
  {
    id: "sg-proposed-after-applied",
    sequence: 2,
    rewriteUnitId: "unit-1",
    beforeText: "原文一",
    afterText: "新的待审阅版本",
    diffSpans: [],
    decision: "proposed",
    slotUpdates: [],
    createdAt: "2026-04-18T10:41:00.000Z",
    updatedAt: "2026-04-18T10:41:00.000Z"
  }
];

const mixedSummary = summarizeRewriteUnitSuggestions(mixedUnitSuggestions);
assert.equal(Boolean(mixedSummary.applied), true);
assert.equal(Boolean(mixedSummary.proposed), true);

const sessionStats = getSessionStats({
  id: "session-1",
  title: "demo",
  documentPath: "demo.docx",
  sourceText: "原文一原文二",
  sourceSnapshot: null,
  normalizedText: "原文一原文二",
  writeBackSupported: true,
  writeBackBlockReason: null,
  plainTextEditorSafe: true,
  plainTextEditorBlockReason: null,
  segmentationPreset: "paragraph",
  rewriteHeadings: false,
  writebackSlots: [],
  rewriteUnits: [
    {
      id: "unit-1",
      order: 0,
      slotIds: [],
      displayText: "原文一",
      segmentationPreset: "paragraph",
      status: "done",
      errorMessage: null
    },
    {
      id: "unit-2",
      order: 1,
      slotIds: [],
      displayText: "原文二",
      segmentationPreset: "paragraph",
      status: "done",
      errorMessage: null
    }
  ],
  suggestions: [
    ...mixedUnitSuggestions,
    {
      id: "sg-proposed",
      sequence: 3,
      rewriteUnitId: "unit-2",
      beforeText: "原文二",
      afterText: "待审阅版本",
      diffSpans: [],
      decision: "proposed",
      slotUpdates: [],
      createdAt: "2026-04-18T10:42:00.000Z",
      updatedAt: "2026-04-18T10:42:00.000Z"
    }
  ],
  nextSuggestionSequence: 4,
  status: "idle",
  createdAt: "2026-04-18T10:39:00.000Z",
  updatedAt: "2026-04-18T10:42:00.000Z"
});

assert.equal(sessionStats.unitsApplied, 1);
assert.equal(
  sessionStats.unitsProposed,
  1,
  "存在已应用 suggestion 的块，不应再同时计入待审阅块"
);

assert.deepEqual(
  buildSuggestionRowActionState({
    suggestionId: "sg-1",
    decision: "dismissed",
    busyAction: null,
    anyBusy: false,
    editorMode: false,
    rewriteRunning: false,
    rewritePaused: false,
    settingsReady: true,
    rewriteUnitFailed: false
  }),
  {
    applyBusy: false,
    applyDisabled: false,
    deleteBusy: false,
    deleteDisabled: false,
    dismissBusy: false,
    dismissDisabled: true,
    retryBusy: false,
    retryDisabled: true,
    retryVisible: false
  }
);

assert.equal(
  shouldScrollToActiveRewriteUnit(
    {
      sessionId: "session-1",
      rewriteUnitId: "unit-1",
      suggestionId: "suggestion-1",
      navigationRequestId: 1
    },
    {
      sessionId: "session-1",
      rewriteUnitId: "unit-1",
      suggestionId: "suggestion-2",
      navigationRequestId: 2
    }
  ),
  true,
  "同一 rewrite unit 下切换 suggestion 时，也应重新定位到左侧正文位置"
);

assert.equal(
  shouldScrollToActiveRewriteUnit(
    {
      sessionId: "session-1",
      rewriteUnitId: "unit-1",
      suggestionId: "suggestion-2",
      navigationRequestId: 2
    },
    {
      sessionId: "session-1",
      rewriteUnitId: "unit-1",
      suggestionId: "suggestion-2",
      navigationRequestId: 3
    }
  ),
  true,
  "即使目标未变化，只要用户再次显式点击定位，也应重新滚动到正文位置"
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

function renderMarkdownMarkup(text) {
  return renderToStaticMarkup(
    React.createElement(
      React.Fragment,
      null,
      renderInlineProtectedText(text, "markdown", "ui-regression")
    )
  );
}

function renderPdfMarkup(text, slot = null) {
  return renderToStaticMarkup(
    React.createElement(
      React.Fragment,
      null,
      renderInlineProtectedText(text, "pdf", "ui-regression", { slot })
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

// 12) Markdown 裸 URL 遇到中文全角标点时，保护区必须只覆盖 URL 本体
const markdownBareUrlMarkup = renderMarkdownMarkup(
  "裸地址 https://example.com/report/final；后面的中文正文不应被一起判成保护区。"
);
assertMatches(
  markdownBareUrlMarkup,
  /<span[^>]*class="inline-protected"[^>]*>https:\/\/example\.com\/report\/final<\/span>；后面的中文正文/,
  "期望 Markdown 裸 URL 在中文全角标点前正确收口"
);

// 13) PDF 占位符文本在无 slot 上下文时也应能高亮
const pdfPlaceholderMarkup = renderPdfMarkup("正文[链接]后文");
assertMatches(
  pdfPlaceholderMarkup,
  /正文<span[^>]*class="inline-protected"[^>]*>\[链接\]<\/span>后文/,
  "期望 PDF 占位符在无 slot 上下文时也能高亮"
);

// 14) PDF slot 携带 protectKind 时，应优先按 slot 保护区渲染
const pdfSlotProtectedMarkup = renderPdfMarkup("[图形]", {
  presentation: { protectKind: "pdf-graphics" }
});
assertMatches(
  pdfSlotProtectedMarkup,
  /<span[^>]*class="inline-protected"[^>]*data-protect-kind="pdf-graphics"[^>]*>\[图形\]<\/span>/,
  "期望 PDF protectKind 来自 slot.presentation，且渲染标记一致"
);

console.log("[ui-regression] OK");
