# 缺陷修复循环记录（TDD）

说明：
- 采用 Red → Green → Refactor 流程：先让测试复现失败，再做最小修复，最后在全部测试通过后重构。
- 主要测试命令（Windows 工具链）：
  - `cmd.exe /c "cd /d E:\Code\LessAI\src-tauri && cargo test -q"`

---

## 2026-03-26 轮次 1（基线：4 个测试失败）

### 失败用例（Red）
- `rewrite::llm::validate::tests::rejects_rewrite_preface_when_source_has_no_preface`
- `adapters::docx::tests::coalesces_softwrapped_paragraph_runs_when_document_looks_line_wrapped`
- `rewrite::tests::sentence_preset_does_not_split_on_numeric_list_marker_period`
- `rewrite::tests::tex_sentence_preset_does_not_split_on_items_without_blank_lines`

### 影响评估
- `validate` 放行“下面是改写后的版本：”会把模型的元话术写进正文，属于高优先级正文污染风险。
- docx 软换行合并失败会导致段落级 chunk 退化为“每行一个块”，审阅体验破碎。
- 数字列表 `2.` 被误当句末会产生碎块，影响 Sentence/Clause 的稳定边界。
- TeX 列表 item 在 Sentence 预设下出现异常，影响 TeX 文本的块内连续性与可读性。

### 修复 1：拒绝“改写引导语”正文污染（Green）
- 缺陷：`validate_rewrite_output()` 未拒绝以“下面是改写后的版本：”开头的候选文本。
- 根因：`starts_with_phrase()` 对中文短语使用了“短语后必须跟空白/标点”的边界判定，导致
  `下面是改写后的版本` 这类“短语后直接接汉字”的场景漏检。
- 覆盖测试：
  - `rewrite::llm::validate::tests::rejects_rewrite_preface_when_source_has_no_preface`
- 修复方案（最小化）：
  - 对 `PREFACE_REWRITE_PREFIX` 仅改为“前缀 starts_with 匹配”，不再走边界字符判定；
    仍保留 `meta_hint`（改写/润色/降重/优化）作为误伤保护。
- 验证：
  - `cargo test -q rewrite::llm::validate::tests::rejects_rewrite_preface_when_source_has_no_preface` 通过。

### 修复 2：docx 软换行段落合并对中文更鲁棒（Green）
- 缺陷：`DocxAdapter::extract_text()` 在“PDF 转 Word：每行一个段落”的 docx 上没有合并，
  导致输出仍包含段落空行分隔符 `\n\n`。
- 根因：软换行边界判定对上一行长度有硬阈值 `MIN_LINE_CHARS = 12`；
  中文行在真实文档里常见 8~12 字的短行，导致合并中途断开。
- 覆盖测试：
  - `adapters::docx::tests::coalesces_softwrapped_paragraph_runs_when_document_looks_line_wrapped`
- 修复方案（最小化）：
  - 在 `should_merge_softwrap_boundary()` 中按脚本自适应最小行长：
    - 含 CJK 字符时使用更低阈值（6）；
    - 否则保持原阈值（12），避免误伤英文文档。
- 验证：
  - `cargo test -q adapters::docx::tests::coalesces_softwrapped_paragraph_runs_when_document_looks_line_wrapped` 通过。

### 修复 3：数字列表 `2.` 不再被误判为句末（Green）
- 缺陷：Sentence 预设在 `1. 第一条。2. 第二条。` 中会把 `2.` 的句号当成句末切分点，
  产生碎块（`2.` 变成独立 chunk）。
- 根因：`is_period_after_numeric_list_marker()` 要求数字前必须是“行首/空白/左括号”，
  但导入文本经常出现 `。2.`（句号后紧接编号、没有空格）的情况，导致漏判。
- 覆盖测试：
  - `rewrite::tests::sentence_preset_does_not_split_on_numeric_list_marker_period`
- 修复方案（最小化）：
  - 放宽“数字前缀允许字符”：在句末标点与常见分隔符（如 `。！？；：` 及其 ASCII 变体）
    后面出现的 `2.` 也视为编号列表标记。
- 验证：
  - `cargo test -q rewrite::tests::sentence_preset_does_not_split_on_numeric_list_marker_period` 通过。

### 修复 4：TeX 列表 `\\item` 不再被保护区切断（Green）
- 缺陷：TeX Sentence 预设下，`\\begin{enumerate} ... \\item 第一项` 会被拆成
  “skip chunk + editable chunk”，导致 `\\item` 命令与其正文被分离（块内连续性差）。
- 根因：
  - `TexAdapter::find_command_span_end()` 会把“命令参数后的行末换行”也吞进命令的 skip span；
  - 相邻的 skip span 会在 `push_region()` 合并；
  - `segment_tex_text::split_tex_top_pieces()` 把“任意多行 skip span”当作 skip block 提前抽离，
    于是 `\\begin{...}\\n\\item ` 变成一个独立的 skip piece，切断同一段的渲染文本流。
- 覆盖测试：
  - `rewrite::tests::tex_sentence_preset_does_not_split_on_items_without_blank_lines`
- 修复方案（最小化）：
  - 调整 `find_command_span_end()`：仅在“空白后紧跟 `[` 或 `{` 参数组”时才吞掉空白；
    不再吞掉“参数之后、且不引出新参数组”的行末换行。
- 验证：
  - `cargo test -q rewrite::tests::tex_sentence_preset_does_not_split_on_items_without_blank_lines` 通过。

### 全量验证（Refactor / 回归）
- `cargo test -q`：78/78 通过。

---

## 2026-03-26 轮次 2（流式解析兼容性）

### 缺陷扫描（分析）
- 风险点：`parse_stream_chat_response_body()` 对 `trimmed.starts_with('{')` 的分支过于激进：
  - 若上游返回“纯文本”但正文刚好以 `{` 开头（例如代码/配置片段），
    会被误判为 JSON/NDJSON 并直接报错；
  - 这会放大“上游 Content-Type/stream 语义不一致”带来的兼容性问题，
    属于高危可用性缺陷（用户无法拿到任何输出）。

### Red：新增失败用例
- 测试：`rewrite::tests::parses_plain_text_stream_body_starting_with_brace`
- 复现：输入 `"{not json}"`，当前实现返回 Err（JSON 解析失败），而不是按纯文本返回。

### Green：最小修复
- 修复点：`src-tauri/src/rewrite/llm/transport.rs`
- 修复策略：
  - 当 `trimmed` 以 `{` 开头但整体 **不符合 NDJSON 形态** 时（`body_looks_like_ndjson` 为 false），
    直接回退为“把内容当纯文本”并走 `sanitize_completion_text()`；
  - 保持 NDJSON / JSON 的原有严格解析逻辑不变，避免误吞真正的结构化响应。

### Refactor / 回归验证
- `cargo test -q`：79/79 通过。

---

## 2026-03-26 轮次 3（分句/整句边界：编号换行）

### 缺陷扫描（分析）
- 风险点：Sentence 模式对编号列表的 `1.` 仅在 “`.` 后紧跟空格/Tab 且同一行有正文” 时才视为列表标记。
- 现实输入：导入 PDF/Word/复制粘贴常见 “编号单独一行，正文下一行”：
  - `1.` 换行
  - `第一条...`
- 当前行为会生成只包含 `1.` / `2.` 的碎块，违背“最小审阅单元应可读”的目标。

### Red：新增失败用例
- 测试：`rewrite::tests::sentence_preset_does_not_split_on_numeric_list_marker_period_followed_by_newline`
- 复现：`"1.\\n第一条。2.\\n第二条。"` 被切成 4 块（`1.` 与正文分离）。

### Green：最小修复
- 修复点：`src-tauri/src/rewrite/segment/boundary.rs`
- 修复策略：
  - 放宽 “`.` 后空白” 判定：允许任意空白（包含换行），只要后面仍有正文内容；
  - 同时收紧 “数字前缀” 判定：允许行首缩进（空格/Tab）以及紧邻/隔空白的句末标点、括号等，
    避免把句中 `他得了 1.` 误判成编号列表。

### Refactor / 回归验证
- `cargo test -q`：80/80 通过。

---

## 2026-03-26 轮次 4（TeX 断句：花括号深度下溢）

### 缺陷扫描（分析）
- 风险点：TeX 的 Sentence/Clause 分块为了“避免切到未闭合 `{...}`”引入了 `brace_depth`。
- 当前实现使用 `i32` 做深度计数；遇到未配对的 `}` 时会变成负数，导致：
  - `brace_depth == 0` 永远不成立；
  - 断句逻辑被永久抑制，整段文本只会生成 1 个 chunk（块过大且边界失效）。
- 这属于明显的边界条件缺陷：输入不完美时应退化为“尽量工作”，而不是彻底失效。

### Red：新增失败用例
- 测试：`rewrite::tests::tex_sentence_preset_does_not_get_stuck_on_unbalanced_braces`
- 复现：`"这是一个}例子。下一句。"` 被错误合并为 1 个 chunk。

### Green：最小修复
- 修复点：`src-tauri/src/rewrite/segment/tex.rs`
- 修复策略：
  - 将 `brace_depth` 类型改为 `usize` 并使用 `saturating_add/sub`，确保深度不会出现负值；
  - 保持“未闭合花括号时延迟切分”的语义不变，只修复下溢导致的永久卡死。

### Refactor / 回归验证
- `cargo test -q`：81/81 通过。

---

## 2026-03-26 轮次 5（英文缩写句末断句：保守增强）

### 缺陷扫描（分析）
- 风险点：Sentence 模式为了避免把英文缩写 `U.S.` / `e.g.` 切碎，会把多点缩写的“末尾 `.`”也视为 token 内部点号。
- 副作用：当缩写实际位于句末（例如 `U.S.A. It ...`），会把两句合并进同一 chunk，粒度偏大。
- 约束：不能引入新碎块（例如把 `U.S. Army` 切成 `U.S.` + `Army ...`），否则会违背“减少过碎切块”的目标。

### Red：新增失败用例
- 测试：`rewrite::tests::sentence_preset_splits_after_ascii_abbreviation_when_it_ends_a_sentence`
- 复现：`"I live in the U.S.A. It is big."` 被错误合并为 1 个 chunk。

### Green：最小修复
- 修复点：`src-tauri/src/rewrite/segment/boundary.rs`
- 修复策略（保守启发式）：
  - 仅对“多点缩写末尾 `.`”做极小的句末识别：
    - 缩写末尾字母为大写（例如 `U.S.` / `U.S.A.` / `Ph.D.`）；
    - 且 `.` 后（跳过空白与闭合符号）紧跟非常常见的英文句首词（`It/This/The/I/We/...` 等）；
  - 满足上述条件时允许在该 `.` 处断句；
  - 增加反例测试：`sentence_preset_does_not_split_after_us_when_followed_by_proper_noun`，确保不把 `U.S. Army` 切碎。

### Refactor / 回归验证
- `cargo test -q`：83/83 通过。

---

## 2026-03-26 轮次 6（中文引号内标点：空白后应断句）

### 缺陷扫描（分析）
- 风险点：`is_punct_quoted_as_literal()` 会把形如 `“？”` / `“，”` 这种“标点被引号包裹”的情况一律视为“提到符号本身”，从而禁止在该标点处断句。
- 问题：在对话/引用场景中，`“？”` 也可能是**真实标点**（例如“一个问号作为回应”），且闭合引号后紧跟空白/换行再开始新句：
  - `他说：“？” 下一句。`
  - `他说：“？”\n下一句。`
- 当前实现会把两句错误合并为 1 个 chunk，影响 Sentence 粒度与审阅可读性。

### Red：新增失败用例
- 测试：`rewrite::tests::sentence_preset_splits_on_quoted_punct_when_followed_by_whitespace`
- 复现：`"他说：“？” 下一句。"` 被错误合并为 1 个 chunk。

### Green：最小修复
- 修复点：`src-tauri/src/rewrite/segment/boundary.rs`
- 修复策略：
  - 在判断“标点被当作字面量符号”前增加一个必要条件：
    - 闭合引号/括号后紧跟的字符 **不能是空白**；
  - 解释：符号提及通常是 `“？”处` / `“，”后面` 这种“紧邻后续文字”的结构；
    而闭合引号后出现空白更像“真实标点后的停顿/换行”，应允许断句。

### Refactor / 回归验证
- `cargo test -q`：84/84 通过。

---

## 2026-03-26 轮次 7（docx 软换行合并：短文档也应触发）

### 缺陷扫描（分析）
- 风险点：`should_enable_softwrap_coalescing()` 只有在候选段落数 `>= 12` 时才启用软换行合并。
- 现实输入：PDF→Word 的短材料（10 行左右）依然会出现“每行一个段落”的硬换行结构；
  若不合并，段落级分块会退化为“每行一个 chunk”，审阅体验很差。

### Red：新增失败用例
- 测试：`adapters::docx::tests::coalesces_softwrapped_paragraph_runs_when_document_looks_line_wrapped_with_fewer_lines`
- 复现：10 行“按行断开”的 docx 未触发合并，输出仍包含 `\\n\\n` 段落分隔符。

### Green：最小修复
- 修复点：`src-tauri/src/adapters/docx.rs`
- 修复策略：
  - 将启发式的最小候选段落数阈值从 `12` 下调到 `8`；
  - 保留其它保守条件（短行比例、终止标点、heading/list 排除）以降低误伤风险。

### Refactor / 回归验证
- `cargo test -q`：85/85 通过。

---

## 2026-03-26 轮次 8（整句断句：称谓缩写 `Dr.` 不应切碎）

### 缺陷扫描（分析）
- 风险点：Sentence/Clause 模式把 ASCII `.` 视为句末边界，但当前只保护了：
  - 多点缩写（`e.g.` / `U.S.A.`）；
  - 文件名/域名（`report.final.v2.pdf`）；
  - 数字列表 `1.` / 小数点等。
- 现实输入：英文称谓缩写（`Dr.` / `Mr.` / `Ms.` / `Prof.`）非常常见：
  - `I met Dr. Smith yesterday.`
  - 若在 `Dr.` 处断句，会产生 `Dr.` 这种不可读碎块，审阅体验很差。

### Red：新增失败用例
- 测试：`rewrite::tests::sentence_preset_does_not_split_on_title_abbreviation_period`
- 复现：`"I met Dr. Smith yesterday. Next sentence."` 被切成 3 块（`Dr.` 独立成块）。

### Green：最小修复
- 修复点：`src-tauri/src/rewrite/segment/boundary.rs`
- 修复策略（保守）：
  - 新增 `is_period_after_title_abbreviation()`：
    - 识别 `Dr./Mr./Ms./Mrs./Prof./Sr./Jr./St.` 等称谓缩写；
    - 默认把该 `.` 视为 token 内部点（不触发断句）；
    - 但如果 `.` 后紧跟非常常见的英文句首词（复用 `looks_like_english_sentence_starter`），
      则仍允许断句（避免把真实句末错误合并）。
  - 将该保护加入 `is_sentence_boundary()` 的 `.` 分支，防止碎块。

### Refactor / 回归验证
- `cargo test -q`：86/86 通过。

---

## 2026-03-26 轮次 9（UI：动作条不应“左滑右滑”，按钮不应尾部裁切）

### 缺陷扫描（分析）
- 风险点：
  - 审阅区与视图切换条存在 `overflow-x: auto` 的“隐藏横向滚动”，导致按钮需要左右滑动才能看全；
  - 文档面板 header 的 action 区域 `flex-shrink: 0`，在窄窗口/字体变大时容易把右侧按钮尾部裁切。
- 期望：按钮固定在可见区域内，不出现“截断”，也不需要横向滚动操作。

### Red：新增失败用例（轻量 UI 回归测试）
- 测试脚本：`scripts/ui-regression.test.mjs`
- 断言：
  - `.workbench-review-panel .workbench-review-actionbar-buttons` 不应使用 `overflow-x: auto`
  - `.review-switches` 不应使用 `overflow-x: auto`
  - `.workbench-doc-panel .panel-action` 必须允许 shrink（`flex: 0 1 auto`）
- 当前状态运行：`node scripts/ui-regression.test.mjs` 失败（命中 `overflow-x: auto` 与不可 shrink）。

### Green：最小修复
- 修复点：
  - `src/styles/part-02.css`
  - `src/styles/part-04.css`
- 修复策略：
  - 移除审阅动作区与 review switches 的横向滚动依赖（改为 `overflow: visible/hidden` + flex 可收缩）；
  - 允许文档面板 header action 区域 shrink，避免右侧按钮被裁切；
  - 同步把文档动作条的容器与 reel 的 `overflow-x` 调整为 `visible`，减少阴影/圆角被裁切的观感。

### Refactor / 回归验证
- `node scripts/ui-regression.test.mjs`：通过。
- `cargo test -q`：86/86 通过。
- （附）清理 clippy dead_code 噪声：将 `DocxAdapter::extract_text` 限定为仅测试可用（不影响导入流程），回归通过。

---

## 2026-03-26 轮次 10（整句断句：`et al.` 不应产生 `al.` 碎块）

### 缺陷扫描（分析）
- 风险点：Sentence/Clause 模式对 ASCII `.` 的保护主要覆盖多点缩写（`e.g.`/`U.S.A.`）与文件名等；
  但学术写作里非常常见的 `et al.`（引用作者群）属于“单点缩写”，会被误当句末断开，
  产生 `al.` 独立 chunk。
- 影响：论文/报告审阅时会出现大量不可读碎块，严重破坏块粒度与可读性。

### Red：新增失败用例
- 测试：`rewrite::tests::sentence_preset_does_not_split_on_et_al_abbreviation_period`
- 复现：`"Smith et al. showed it. Next sentence."` 被切成 3 块（`al.` 独立成块）。

### Green：最小修复
- 修复点：`src-tauri/src/rewrite/segment/boundary.rs`
- 修复策略（保守）：
  - 将原来的称谓缩写保护泛化为 `is_period_after_common_abbreviation()`，
    在既有 `Dr./Mr./Prof.` 等基础上新增 `al.`；
  - 同样复用 `looks_like_english_sentence_starter`：当它看起来确实位于句末时仍允许断句，
    避免无脑合并造成超大块。

### Refactor / 回归验证
- `cargo test -q`：87/87 通过。

---

## 2026-03-26 轮次 11（整句断句：论文常见缩写 `Fig./Eq./Sec.` 不应切碎）

### 缺陷扫描（分析）
- 风险点：学术文档里大量出现引用缩写：
  - `Fig. 1` / `Eq. (3)` / `Sec. 2`
- 当前实现会把 `Fig.` / `Eq.` / `Sec.` 的 `.` 当句末，导致：
  - `Fig.` 独立成块
  - 后续数字/括号内容变成下一块开头
- 影响：碎块数量激增，且碎块不可读，严重破坏审阅体验。

### Red：新增失败用例
- 测试：
  - `rewrite::tests::sentence_preset_does_not_split_on_fig_abbreviation_period`
  - `rewrite::tests::sentence_preset_does_not_split_on_eq_abbreviation_period`
  - `rewrite::tests::sentence_preset_does_not_split_on_sec_abbreviation_period`
- 复现：`"See Fig. 1 for details. Next sentence."` 被切成 3 块（`Fig.` 独立成块）。

### Green：最小修复
- 修复点：`src-tauri/src/rewrite/segment/boundary.rs`
- 修复策略：
  - 扩展 `is_period_after_common_abbreviation()` 的缩写白名单，加入 `fig/eq/sec`；
  - 仍保持“句末识别保守”：若后面紧跟常见英文句首词，则仍允许断句，避免无脑合并。

### Refactor / 回归验证
- `cargo test -q`：90/90 通过。

---

## 2026-03-26 轮次 12（整句断句：`Ref./No./Vol./Ch.` 等缩写不应切碎）

### 缺陷扫描（分析）
- 风险点：论文/报告引用中还常见：
  - `Ref. [1]`
  - `No. 1`
  - `Vol. 2`
  - `Ch. 3`
- 当前实现会在这些缩写的 `.` 处断句，产生 `Ref.` / `No.` / `Vol.` / `Ch.` 碎块。

### Red：新增失败用例
- 测试：
  - `rewrite::tests::sentence_preset_does_not_split_on_ref_abbreviation_period`
  - `rewrite::tests::sentence_preset_does_not_split_on_no_abbreviation_period`
  - `rewrite::tests::sentence_preset_does_not_split_on_vol_abbreviation_period`
  - `rewrite::tests::sentence_preset_does_not_split_on_ch_abbreviation_period`
- 复现：`"See Ref. [1] for details. Next sentence."` 被切成 3 块（`Ref.` 独立成块）。

### Green：最小修复
- 修复点：`src-tauri/src/rewrite/segment/boundary.rs`
- 修复策略：
  - 扩展 `is_period_after_common_abbreviation()` 白名单，加入 `ref/no/vol/ch`；
  - 继续复用“常见句首词”启发式：看起来确实句末时仍允许断句。

### Refactor / 回归验证
- `cargo test -q`：94/94 通过。
