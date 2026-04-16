# Unified Rewrite Unit & Safe Writeback Design

## Goal

将所有支持的文档格式统一到同一条闭环主链，严格满足以下约束：

- `1 个用户可见分块 = 1 次 LLM 调用`
- `1 次调用 = 1 个结构化返回结果`
- `1 个结果 = 1 条 suggestion / 1 个审阅项`
- 能被 AI 改写的内容，必须能进入同一条安全写回链
- 结构校验失败时，整块失败；不部分落地，不 silent fallback

## Confirmed Constraints

- 分块规则由用户设置决定，分块结果必须成为真实调用单元，而不是仅前端视觉分组。
- 所有格式都要满足“一个分块只调用一次 LLM”。
- 所有格式都要满足安全写回；不能通过弱化写回校验来换取“看起来能跑”。
- 当一个块同时包含可改写文本与不可改写内容时，LLM 仍只调用一次，但必须返回带锚点的结构化结果。
- 如果 LLM 返回无法通过结构校验或无法安全写回，该块直接失败，不写回、不部分成功、不自动降级。
- 不做 silent fallback、mock 成功路径、兼容层兜底。

## Problem

当前实现中，`session.chunks` 同时承担了三种职责：

1. 用户可见分块
2. LLM 调用单元
3. 安全写回边界单元

这在纯文本场景下勉强成立，但在 `docx`、`md`、`tex` 中会直接错位：

- 前端会把多个底层 chunk regroup 成一个可见块。
- 后端实际仍按底层 chunk 选择目标、发起改写、生成 suggestion。
- 写回时又依赖底层样式/语法/锁定边界做安全校验。

结果就是：

- 一个可见块可能需要多次 LLM 调用才能“处理完”
- 一个可见块可能生成多条 diff / 多条 suggestion
- “可改写”和“可写回”之间出现错位
- 编辑器、审阅、自动改写、写回校验之间并不共享同一主键

## Design Overview

采用双层模型：

- 底层保留最小安全写回真相：`WritebackSlot`
- 上层引入真正的用户分块与调用单元：`RewriteUnit`

主流程统一为：

1. 导入文档
2. 解析为 `WritebackSlot[]`
3. 基于用户分块设置生成 `RewriteUnit[]`
4. 用户选择 `RewriteUnit`
5. 对该 unit 发起一次 LLM 调用
6. LLM 返回结构化 `SlotUpdate[]`
7. 生成一条 `RewriteSuggestion`
8. 基于 `SlotUpdate[]` 做格式相关安全校验
9. 校验通过后写回；失败则整 unit 失败

## Core Data Model

### WritebackSlot

最小安全写回单元。所有格式都统一映射到这层。

关键字段：

- `id`: 稳定标识，写回和刷新必须依赖它而不是数组下标
- `order`: 文档内顺序
- `text`: 当前展示文本
- `editable`: 是否允许改写
- `role`: 统一角色类型
- `presentation`: 样式/链接/保护信息
- `anchor`: 格式特定写回锚点
- `separator_after`: 槽位后的分隔文本

### RewriteUnit

用户可见块，也是唯一 LLM 调用单元。

关键字段：

- `id`
- `order`
- `slot_ids`: 本块覆盖的有序 slot 集合
- `display_text`: 面向用户展示的块文本
- `chunk_preset`: 当前块对应的分块策略
- `status`
- `error_message`

### SlotUpdate

块级结构化返回中的最小更新单元。

关键字段：

- `slot_id`
- `text`

### RewriteSuggestion

一次块级改写结果。

关键字段：

- `id`
- `sequence`
- `rewrite_unit_id`
- `before_text`
- `after_text`
- `diff_spans`
- `decision`
- `slot_updates`
- `created_at`
- `updated_at`

## Unified Slot Roles

所有格式共享同一套基础角色：

- `EditableText`
- `LockedText`
- `SyntaxToken`
- `InlineObject`
- `ParagraphBreak`

格式映射规则：

- `txt`: 以 `EditableText` 为主，换行/段落分隔映射为 `ParagraphBreak`
- `md`: 正文文字为 `EditableText`，语法标记为 `SyntaxToken`
- `tex`: 正文文字为 `EditableText`，命令/环境/数学边界为 `SyntaxToken` 或 `LockedText`
- `docx`: 正文文字与超链接显示文字为 `EditableText`，公式/图片/内容控件/分页符/占位符为 `LockedText` 或 `InlineObject`

## LLM Contract

### Input

一次调用只处理一个 `RewriteUnit`。输入不是裸文本，而是“完整块 + 结构槽位表”。

逻辑结构：

```json
{
  "rewrite_unit_id": "unit-43",
  "format": "docx",
  "display_text": "请访问官网下载 [图表]",
  "slots": [
    { "slot_id": "s1", "role": "EditableText", "editable": true, "text": "请访问" },
    { "slot_id": "s2", "role": "EditableText", "editable": true, "text": "官网" },
    { "slot_id": "s3", "role": "EditableText", "editable": true, "text": "下载" },
    { "slot_id": "s4", "role": "InlineObject", "editable": false, "text": "[图表]" }
  ]
}
```

### Output

LLM 必须返回结构化结果，不接受自由文本整段替换。

逻辑结构：

```json
{
  "rewrite_unit_id": "unit-43",
  "updates": [
    { "slot_id": "s1", "text": "请前往" },
    { "slot_id": "s2", "text": "官方网站" },
    { "slot_id": "s3", "text": "获取资料" }
  ]
}
```

硬约束：

- 必须返回匹配的 `rewrite_unit_id`
- 只能更新当前 unit 中 `editable = true` 的 slot
- 不能返回未知 `slot_id`
- 不能修改 `LockedText` / `SyntaxToken` / `InlineObject`
- 缺字段、重复 slot、非法 JSON、非法顺序时整块失败

## Unified Runtime Flow

### Import

各格式适配器统一产出：

- `WritebackSlot[]`
- 格式相关 `DocumentStructure`

适配器职责仅限于：

- 提取原始结构真相
- 标注哪些内容可改写
- 提供安全写回所需 anchor

### Segmentation

分块设置只作用于 `slot -> unit`，不再直接作用于底层写回边界。

要求：

- 一个 `RewriteUnit` 从生成开始就是真实用户块
- 后端不得再把该 unit 拆成多个调用单元
- 文本格式可按语法定制切分器，但输出必须统一成 `RewriteUnit[]`

### Rewrite

- 手动模式：一次选择一个 `RewriteUnit`
- 自动模式：允许并发多个 unit，但每个 unit 仍是一次独立调用
- 批处理设置若保留，只能表示“一次请求可承载几个 unit”，不能表示“一个 unit 内拆几个子块”

### Review

审阅与 suggestion 全部绑定 `rewrite_unit_id`：

- 一个 unit 一次改写只生成一条 suggestion
- 审阅面板按 unit 展示
- diff 以 unit 级 `before_text/after_text` 展示
- 底层 `slot_updates` 只用于安全写回与回放

### Editor

编辑器与 AI 改写共用同一套写回模型：

- 编辑器展示单位也是 `RewriteUnit`
- 编辑结果也要转成 `SlotUpdate[]`
- 编辑器与 AI 共用同一套结构校验器

最终只允许两种输入源不同：

- AI 模式：`SlotUpdate[]` 来自 LLM 返回
- 编辑模式：`SlotUpdate[]` 来自用户手动修改

### Writeback

写回层不再依赖“从整段字符串回推边界”，而是：

1. 验证 `SlotUpdate[]` 只覆盖允许编辑的 slot
2. 基于 slot_id 投影回原始结构
3. 进行格式特定安全验证
4. 验证通过后写回原文件

失败语义统一为：

- suggestion 不落地
- 文档不写回
- 当前 unit 进入失败状态并保留错误信息

## Format-Specific Safety Rules

### txt

- 更新后的文本重建必须与 unit/slot 拼接规则一致

### md

- `SyntaxToken` 不得被修改
- 仅允许替换正文 `EditableText`
- 重建后必须仍能按原有语法边界还原

### tex

- 命令、环境、数学边界和转义结构不得被修改
- 仅允许替换正文 `EditableText`
- 重建后必须保持原有语法结构闭合

### docx

- 更新只允许落在可编辑 slot 上
- 超链接、样式、锁定区、占位符 anchor 不得漂移
- 不再依赖字符串猜测 run/hyperlink/locked 边界

## Module Plan

建议重组为以下模块：

- `adapters/*`: 各格式仅负责解析/重建 `WritebackSlot`
- `rewrite/unit`: 负责 `slot -> unit` 分块
- `rewrite/protocol`: 负责 unit 级请求编码、响应解析与 schema 校验
- `rewrite/suggestion`: 负责 unit 级 suggestion 与 diff
- `rewrite/writeback`: 负责 `SlotUpdate[] -> validate -> write`
- `session/*`: 会话状态统一绑定 `rewrite_unit_id`
- `frontend document/review/editor`: 直接消费 `RewriteUnit`

## Migration Order

1. 引入 `WritebackSlot`、`RewriteUnit`、`SlotUpdate`、`RewriteSuggestion`
2. 将所有导入器改为产出 slot，而不是旧 `ChunkTask`
3. 用新的分块层生成 `RewriteUnit[]`
4. 将 LLM 协议切到 unit 级结构化输入/输出
5. 将 suggestion、选择、审阅、任务状态切到 `rewrite_unit_id`
6. 将编辑器和写回统一到 `SlotUpdate[]` 主链
7. 删除旧的 regroup、`chunk_index` suggestion、自由文本返回协议与相关兼容代码

## Risks

- `docx` 的 `slot_id`/anchor 稳定性不足，会导致刷新和写回对位失败
- `md/tex` 语法 token 提取不完整，会出现“误判可改写”的漏洞
- 旧 session 数据无法安全迁移，必须明确要求重新导入
- 如果前后端任意一层仍偷偷保留 `chunk_index` 真相，会再次出现视觉块与调用单元错位

## Validation

必须验证以下硬规则：

- `1 个 RewriteUnit = 1 次 LLM 调用`
- `1 次调用 = 1 个结构化结果`
- `1 个结果 = 1 条 suggestion`
- 可改写 unit 一定能进入同一条安全写回链
- 写回失败时整 unit 失败，不部分成功
- 锁定/语法/占位 slot 绝不被非法修改

测试分组：

1. 导入测试：验证每种格式的 slot 提取与 unit 生成
2. 协议测试：验证非法 JSON、非法 slot、非法 unit_id、修改锁定 slot 的失败行为
3. 写回测试：验证成功样例与结构非法样例
4. 端到端测试：验证“选择 -> 调用 -> suggestion -> 审阅 -> 写回”全链一致

## Non-Goals

- 不追求兼容旧 session 数据
- 不保留旧 `chunk_index` 主链作为 fallback
- 不通过降低写回校验来换取“更多块能过”
- 不把前端 regroup 继续留作正式逻辑

## Result

该设计将当前“前端看起来是一块、后端实际是多块”的中间态，收敛为一套统一真相：

- 用户分块是真实调用单元
- 写回边界是真实结构单元
- 两者通过结构化协议和 `SlotUpdate[]` 连接

这样才能同时满足：

- 一个分块只调用一次 LLM
- 一个分块只生成一条结果
- 一个分块可严格安全写回
