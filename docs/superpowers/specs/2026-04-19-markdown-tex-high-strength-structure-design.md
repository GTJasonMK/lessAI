# Markdown/TeX High-Strength Structured Support Design

## Goal

将 `markdown / tex` 提升到与当前 `docx` 同等级的高强度结构化支持，满足以下闭环目标：

- 分块先基于格式结构，再基于统一分段规则生成用户块
- `1 个用户块 = 1 个 rewrite unit`
- 设置里的 `单批处理单元数` 严格等于 `1 次模型调用承载的 rewrite unit 数`
- suggestion 的主真相是 `slot_updates`，不是整段 after text
- 可改写区与可安全写回区必须同源，不允许前后链路判定分叉
- 结构变化时必须显式失败，不做 silent fallback、兼容回退或猜测性迁移

本设计不扩展新的 Markdown/TeX 语法支持范围，只把当前已经支持识别的结构提升为严格闭环主链。

## Confirmed Constraints

- 不增加 fallback、兼容兜底、模糊对齐或假成功路径
- 不新增 Markdown/TeX 语法支持面，只迁移当前已支持能力
- 用户看到的块边界允许变化，但只能因为语法结构更正确而变化，不能为了实现方便漂移
- Markdown/TeX 的语法壳必须像 `docx` 锁定区一样，在前端明确显示为不可改写区
- 对不完整或损坏的语法，只锁定“能确定识别”的语法壳；不能确定的部分按普通文本处理
- 一旦模板签名、slot 结构签名或写回边界校验失败，必须显式报错并阻断继续写回

## Problem

当前系统已经统一到 `WritebackSlot[] -> RewriteUnit[]` 的后半段主链，但 `markdown / tex` 仍未达到 `docx` 的闭环强度：

- 结构识别和最终写回之间还存在旧桥接层
- 语法壳虽已在部分后端逻辑中识别，但还没有全部稳定落成前端可见的 locked slot
- 选区改写、刷新、suggestion 提交、最终写回仍有“结构真相不唯一”的风险
- 文本格式仍保留了“整篇文本覆盖”这类旧路径，导致“能改写”和“能安全写回”未完全收口

结果是：

- `markdown / tex` 还只是“比纯文本更懂语法”，不是“高强度结构化支持”
- 旧链一旦被触发，就可能再次引入块边界漂移、锁定边界漂移或写回失败
- 与 `docx` 相比，文本格式的结构真相还不够硬

## Chosen Approach

采用“在现有 `markdown / tex` 解析能力之上补齐 `docx` 同构闭环”的方案，不引入外部 AST，不保留双轨运行。

统一主链为四段：

1. `FormatTemplate`
   各格式先生成稳定模板，而不是直接生成展示块或临时字符串切片
2. `AnchoredSlots`
   模板转换为带稳定 `anchor` 的 `WritebackSlot`
3. `RewriteUnits`
   统一由 `rewrite_unit/build.rs` 在 slot 流上生成用户块
4. `SlotWriteback`
   suggestion、写回、selection rewrite 都只通过 slot 真相工作

这条链中：

- `template_snapshot` 是格式结构真相
- `template_signature` 校验模板骨架是否稳定
- `slot_structure_signature` 校验 slot 边界和顺序是否稳定
- `rewrite_units` 只是视图层组合，不再承载结构真相

## Markdown Template Model

Markdown 仅基于当前已支持识别的结构生成模板，不新增语法支持面。

### Block Types

- `paragraph`
- `heading`
- `quote`
- `list_item`
- `locked_block`

### Locked Block Sources

- front matter
- fenced code
- math block
- table
- html block
- reference definition

### Region Types

- `editable_region`
- `locked_region`

### Locked Region Sources

- inline code 壳
- link 语法壳
- bare URL
- inline math 壳
- inline HTML 壳
- emphasis delimiter 等必须保真的分隔符

### Anchor Rules

- block anchor: `md:b{block_index}`
- region anchor: `md:b{block_index}:r{region_index}`
- slot split anchor: `md:b{block_index}:r{region_index}:s{split_index}`

约束：

- anchor 只能来自“语法结构路径 + 顺序位置”
- 不能基于文本内容计算
- 同一份文档在结构不变时，重复导入必须得到稳定 anchor

## TeX Template Model

TeX 同样只基于当前已支持识别的结构生成模板，不扩语法面。

### Block Types

- `paragraph`
- `command_block`
- `environment_block`
- `math_block`
- `locked_block`

### Locked Block Sources

- comment block
- raw environment
- math environment
- `verbatim` / `minted` / `lstlisting` 等原样区
- 当前已判定不可安全改写的命令壳或环境壳

### Region Types

- `editable_region`
- `locked_region`

### Locked Region Sources

- command shell
- environment shell
- math delimiter shell
- comment shell
- URL shell
- 必须保真的参数壳

对当前已经支持“只开放正文参数”的文本型命令，保持：

- 命令壳 locked
- 参数正文 editable

### Anchor Rules

- block anchor: `tex:b{block_index}`
- region anchor: `tex:b{block_index}:r{region_index}`
- slot split anchor: `tex:b{block_index}:r{region_index}:s{split_index}`

## Unified Runtime Closure

### Import

导入时统一执行：

1. 解析格式模板
2. 生成 anchored slots
3. 计算 `template_signature`
4. 计算 `slot_structure_signature`
5. 基于 slots 构建 `rewrite_units`
6. 将模板快照、签名、slots、rewrite units 一并写入 `DocumentSession`

### Refresh

`session refresh` 不再只比较拼接文本，而是统一依赖模板和 slot 结构：

- `Keep`
  模板签名、slot 结构签名、rewrite unit 结构都一致
- `Rebuild`
  结构可稳定重建，且当前 session 干净，没有 suggestion 或活动任务
- `Block`
  结构变化且当前 session 已有 suggestion 或活动任务

不做模糊迁移。

### Suggestion

suggestion 的主真相统一为：

- `rewrite_unit_ids`
- `slot_updates`
- `before/after` 展示投影

存储层不再依赖整段 after text；after/before 只是展示投影。

### Writeback

写回前统一执行：

1. 重新加载源文件
2. 重新生成模板
3. 重新生成 anchored slots
4. 校验 `template_signature`
5. 校验 `slot_structure_signature`
6. 校验更新只作用于当前允许修改的 editable slot
7. 校验更新未跨越任何 locked 边界
8. 按模板 + 更新后的 slots 重建最终文本
9. validate 模式只校验，write 模式原子写盘

任何失败都必须显式报错，不允许回退到整篇文本覆盖兜底。

### Selection Rewrite

选区改写也必须进入同一主链：

- 先按格式生成 selection-scoped template
- 再生成 selection-scoped slots
- 再生成 selection-scoped rewrite units
- 最终仍返回 `slot_updates`

不再允许选区改写走临时纯文本切片链。

### Batch Semantics

- `1 个用户块 = 1 个 rewrite unit`
- `1 个 batch = 1 次模型调用`
- `units_per_batch = 单次模型调用承载的 rewrite unit 数`

因此：

- 单块改写是一条 unit 的 batch
- 多块批量改写是多 unit 的 batch
- 进度、失败、状态、展示都必须以 batch 为一级真相，不能再把 unit 数冒充 batch 数

## Module Plan

### Shared Textual Infrastructure

继续使用并补强：

- `src-tauri/src/textual_template/mod.rs`
- `src-tauri/src/textual_template/models.rs`
- `src-tauri/src/textual_template/signature.rs`
- `src-tauri/src/textual_template/slots.rs`
- `src-tauri/src/textual_template/rebuild.rs`
- `src-tauri/src/textual_template/validate.rs`

### Markdown

将当前超长单文件拆分为目录模块：

- `src-tauri/src/adapters/markdown/mod.rs`
- `src-tauri/src/adapters/markdown/blocks.rs`
- `src-tauri/src/adapters/markdown/inline.rs`
- `src-tauri/src/adapters/markdown/template.rs`
- `src-tauri/src/adapters/markdown/tests.rs`

### TeX

将当前超长单文件拆分为目录模块：

- `src-tauri/src/adapters/tex/mod.rs`
- `src-tauri/src/adapters/tex/blocks.rs`
- `src-tauri/src/adapters/tex/commands.rs`
- `src-tauri/src/adapters/tex/template.rs`
- `src-tauri/src/adapters/tex/tests.rs`

### Mainline Consumers

需要切主链的后端模块：

- `src-tauri/src/documents/source.rs`
- `src-tauri/src/documents/writeback.rs`
- `src-tauri/src/session_builder.rs`
- `src-tauri/src/session_refresh.rs`
- `src-tauri/src/session_refresh/rules.rs`
- `src-tauri/src/rewrite_writeback.rs`
- `src-tauri/src/rewrite/llm/selection.rs`

## Deletions

本轮不保留旧生产链长期并存，以下旧链会被删除或从主链中移除：

- `src-tauri/src/adapters/mod.rs` 中的扁平 `TextRegion` 生产主真相
- `src-tauri/src/adapters/markdown_template.rs`
- `src-tauri/src/adapters/tex_template.rs`
- `src-tauri/src/adapters/textual_regions_template.rs`
- `selection rewrite` 中“直接基于 source_text 临时 build slots + plain finalize”的旧路径
- 文本格式“整篇文本覆盖是主路径，slot 写回只是补充”的残余分叉

## Testing and Verification

至少需要四层测试：

### 1. Template Tests

- 块类型、region 类型、anchor 顺序稳定
- 同一输入重复解析，`template_signature` 稳定
- 语法壳必须落成 locked region

### 2. Slot Tests

- slot 顺序、role、separator、anchor 稳定
- `slot_structure_signature` 能感知结构变化
- locked slot 不可更新，editable slot 可更新

### 3. Rebuild and Writeback Tests

- 模板 + 原始 slots 重建结果必须与源文本完全一致
- 只改 editable slot 时，只改正文，不改语法壳
- 跨越 locked 边界、anchor 漂移、slot 重排必须显式失败

### 4. Session and Rewrite Flow Tests

- `DocumentSession` 正确持久化模板快照和签名
- refresh 的 `Keep / Rebuild / Block` 稳定
- 手动改写、自动改写、批量改写、selection rewrite 都最终走同一 slot 提交与写回链

## Completion Criteria

只有当以下条件同时成立，才算完成本设计：

- `markdown / tex` 导入后都能生成稳定模板、稳定 anchored slots、稳定 rewrite units
- 前端能明确显示 locked 语法壳
- `1 个用户块 = 1 个 rewrite unit`
- `单批处理单元数 = 1 次模型调用承载的 unit 数`
- suggestion 主真相是 `slot_updates`
- 文本格式最终写回主路径是 slot writeback
- 模板签名 / slot 结构 / 写回边界异常都能显式失败

## Non-Goals

- 不处理 PDF 原位安全写回
- 不扩展新的 Markdown/TeX 语法识别能力
- 不为了兼容旧展示行为而保留第二套分块或写回链
