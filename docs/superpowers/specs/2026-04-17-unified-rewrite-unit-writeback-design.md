# Unified Rewrite Batch, Unit, and Safe Writeback Design

## Goal

将所有支持的文档格式统一到同一条闭环主链，严格满足以下约束：

- `1 个 batch = 1 次 LLM 调用`
- `1 个 batch 含 N 个用户可见分块`
- `1 个用户可见分块 = 1 条 suggestion / 1 个审阅项`
- 能被 AI 改写的内容，必须能进入同一条安全写回链
- 结构校验失败时，整批失败；不部分落地，不 silent fallback

这里的 `batch` 明确定义为设置项 `单批处理单元数`（`units_per_batch`）所表示的一次模型请求承载的分块集合。

## Confirmed Constraints

- 分块规则由用户设置决定，分块结果必须成为真实调用单元，而不是仅前端视觉分组。
- `units_per_batch` 的真实语义必须等于“一次模型调用里包含多少个用户分块”。
- `max_concurrency` 的真实语义必须等于“同时允许多少个 batch 请求在飞”。
- 所有格式都要满足安全写回；不能通过弱化写回校验来换取“看起来能跑”。
- 当一个块同时包含可改写文本与不可改写内容时，LLM 仍只通过结构化协议返回结果，且只能更新允许改写的槽位。
- 如果 LLM 返回无法通过结构校验或无法安全写回，该 batch 直接失败，不写回、不部分成功、不自动降级。
- 不做 silent fallback、mock 成功路径、兼容层兜底。

## Problem

当前实现已经引入了 `WritebackSlot` 和 `RewriteUnit`，解决了“可见块”和“安全写回边界”分离的问题，但 `units_per_batch` 仍未成为真实的协议语义。

当前批处理链路的真实行为是：

1. 先按 `units_per_batch` 取出多个 `RewriteUnit`
2. 组成一个“调度批次”
3. 再对批次中的每个 unit 顺序发起一次单独的模型调用
4. 最后把这一批返回结果一起提交

结果就是：

- 设置中的“单批处理单元数”并不等于“一次模型调用里有几个块”
- `max_concurrency` 和 `units_per_batch` 都在做调度分组，语义重叠
- 所谓 batch 只是“队列分组 + 提交分组”，不是“推理分组”
- 用户理解的“批量优化”与实际执行的“逐块调用”不一致

## Design Overview

采用三层模型：

- 底层保留最小安全写回真相：`WritebackSlot`
- 中层保留真正的用户分块与审阅单元：`RewriteUnit`
- 上层新增真实的调用协议单元：`RewriteBatch`

三者职责严格分离：

- `WritebackSlot`：安全写回边界
- `RewriteUnit`：用户可见块、选择块、diff 块、suggestion 块
- `RewriteBatch`：一次模型调用承载的 unit 集合

主流程统一为：

1. 导入文档
2. 解析为 `WritebackSlot[]`
3. 基于用户分块设置生成 `RewriteUnit[]`
4. 基于 `units_per_batch` 从 `RewriteUnit[]` 中组装 `RewriteBatchRequest`
5. 对该 batch 发起一次 LLM 调用
6. LLM 返回 `RewriteBatchResponse`
7. 对整个 batch 做结构校验与安全写回校验
8. 校验全部通过后，再按 unit 生成多条 `RewriteSuggestion`
9. suggestion 最终通过 `SlotUpdate[]` 走统一写回链

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

用户可见块，也是最小 suggestion 单元。

关键字段：

- `id`
- `order`
- `slot_ids`: 本块覆盖的有序 slot 集合
- `display_text`: 面向用户展示的块文本
- `segmentation_preset`: 当前块对应的分块策略
- `status`
- `error_message`

### SlotUpdate

结构化返回中的最小更新单元。

关键字段：

- `slot_id`
- `text`

### RewriteSuggestion

一次 unit 级改写结果。

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

### RewriteBatchRequest

一次模型调用的请求载体。

关键字段：

- `batch_id`
- `format`
- `units: RewriteUnitRequest[]`

### RewriteBatchResponse

一次模型调用的结构化返回。

关键字段：

- `batch_id`
- `results: RewriteUnitResponse[]`

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
- `docx`: 正文文字与超链接显示文字为 `EditableText`，公式、图片、内容控件、分页符、占位符为 `LockedText` 或 `InlineObject`

## LLM Contract

### Input

一次调用处理一个 `RewriteBatchRequest`，而不是单个 `RewriteUnit`。

逻辑结构：

```json
{
  "batchId": "batch-7",
  "format": "docx",
  "units": [
    {
      "rewriteUnitId": "unit-43",
      "displayText": "请访问官网下载 [图表]",
      "slots": [
        { "slotId": "s1", "role": "EditableText", "editable": true, "text": "请访问" },
        { "slotId": "s2", "role": "EditableText", "editable": true, "text": "官网" },
        { "slotId": "s3", "role": "EditableText", "editable": true, "text": "下载" },
        { "slotId": "s4", "role": "InlineObject", "editable": false, "text": "[图表]" }
      ]
    },
    {
      "rewriteUnitId": "unit-44",
      "displayText": "请填写申请日期",
      "slots": [
        { "slotId": "s5", "role": "EditableText", "editable": true, "text": "请填写" },
        { "slotId": "s6", "role": "EditableText", "editable": true, "text": "申请日期" }
      ]
    }
  ]
}
```

### Output

LLM 必须返回 batch 级结构化结果，不接受自由文本整段替换。

逻辑结构：

```json
{
  "batchId": "batch-7",
  "results": [
    {
      "rewriteUnitId": "unit-43",
      "updates": [
        { "slotId": "s1", "text": "请前往" },
        { "slotId": "s2", "text": "官方网站" },
        { "slotId": "s3", "text": "获取资料" }
      ]
    },
    {
      "rewriteUnitId": "unit-44",
      "updates": [
        { "slotId": "s5", "text": "请补充" },
        { "slotId": "s6", "text": "申报日期" }
      ]
    }
  ]
}
```

硬约束：

- 必须返回匹配的 `batch_id`
- `results` 数量必须与请求中的 unit 数量一致
- `results` 中的 `rewrite_unit_id` 必须与请求逐一对应，不得缺失、重复或乱序
- 每个 unit 只能更新自己范围内 `editable = true` 的 slot
- 不能返回未知 `slot_id`
- 不能修改 `LockedText`、`SyntaxToken`、`InlineObject`
- 同一 batch 内不同 unit 不得更新同一个 slot
- 缺字段、重复 slot、非法 JSON、非法顺序时整批失败

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

### Batch Assembly

`units_per_batch` 只作用于 `unit -> batch`。

要求：

- 手动模式：从选中的可改写 unit 中取最多 `units_per_batch` 个，组装一个 `RewriteBatchRequest`
- 自动模式：从 pending 队列中每次取最多 `units_per_batch` 个，组装一个 `RewriteBatchRequest`
- 一个 batch 只对应一次模型调用
- 一个 batch 的成功或失败必须按原子事务处理

### Rewrite

- 手动模式：一次执行发起一个 batch 请求
- 自动模式：允许并发多个 batch 请求，但每个 batch 仍只调用一次模型
- `max_concurrency` 只控制 in-flight batch 数，不控制单个 batch 内的 unit 数

### Review

审阅与 suggestion 全部绑定 `rewrite_unit_id`：

- 一个 unit 一次改写只生成一条 suggestion
- 一个 batch 成功后可以生成多条 suggestion，但这些 suggestion 属于同一次 batch 提交
- 审阅面板按 unit 展示
- diff 以 unit 级 `before_text` / `after_text` 展示
- 底层 `slot_updates` 只用于安全写回与回放

### Editor

编辑器与 AI 改写共用同一套写回模型：

- 编辑器展示单位仍是 `RewriteUnit`
- 编辑结果也要转成 `SlotUpdate[]`
- 编辑器与 AI 共用同一套结构校验器

最终只允许两种输入源不同：

- AI 模式：`SlotUpdate[]` 来自 batch 返回中的 unit 结果
- 编辑模式：`SlotUpdate[]` 来自用户手动修改

### Writeback

写回层不再依赖“从整段字符串回推边界”，而是：

1. 验证 batch 内每个 unit 的 `SlotUpdate[]` 只覆盖允许编辑的 slot
2. 验证 batch 内 unit 之间不存在 slot 冲突
3. 基于 slot_id 投影回原始结构
4. 进行格式特定安全验证
5. 验证全部通过后再允许提交 suggestion 与最终写回

失败语义统一为：

- 当前 batch 的 suggestion 全部不落地
- 文档不写回
- 当前 batch 中所有 unit 进入失败状态并保留同一批次错误信息

## Failure Semantics

采用整批原子失败，不允许部分落地。

任一以下情况成立时，整个 batch 失败：

- `batch_id` 不匹配
- `results` 数量不一致
- 某个 `rewrite_unit_id` 缺失、重复、顺序错乱
- 某个 result 越过本 unit 的 slot 边界
- 某个 result 修改了 locked / syntax / inline object slot
- 某个 result 命中了未知 slot
- 不同 unit 更新了同一 slot
- 任意 result 在格式特定安全写回校验中失败

失败后不允许：

- 生成部分 suggestion
- 部分提交 session
- 部分写回文档

## Commit Semantics

batch 成功时，仍然按 unit 逐个生成 suggestion，但生成和提交必须发生在同一次 session mutation 中。

语义为：

- `1 个 batch = 1 次提交事务`
- `1 个 unit = 1 条 suggestion`
- `1 次事务成功 = 该 batch 内全部 suggestion 一起落地`

手动模式：

- batch 成功且 `auto_approve = false` 时，整批生成 `Proposed` suggestion
- 任一 unit 不合法时，整批报错，0 条 suggestion 落地

自动模式：

- batch 成功后，整批内所有 unit suggestion 直接 `Applied`
- batch 失败时，该批内所有 unit 都标记为 `Failed`

## Format-Specific Safety Rules

### txt

- 更新后的文本重建必须与 unit / slot 拼接规则一致

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
- 不再依赖字符串猜测 run / hyperlink / locked 边界

## Module Plan

建议重组为以下模块：

- `adapters/*`: 各格式仅负责解析 / 重建 `WritebackSlot`
- `rewrite_unit/build`: 负责 `slot -> unit` 分块
- `rewrite_unit/protocol`: 负责 unit 级请求、batch 级协议和 schema 校验
- `rewrite/llm`: 负责 batch 级调用与 transport
- `rewrite_batch_commit`: 负责 batch 原子校验与 unit 级 suggestion 提交
- `rewrite_writeback`: 负责 batch 级安全写回验证
- `session/*`: 会话状态统一绑定 `rewrite_unit_id`
- `frontend document / review / editor`: 直接消费 `RewriteUnit`

## Migration Order

1. 将 LLM 协议从“单 unit 请求/响应”升级为“batch 请求/响应”
2. 将手动与自动改写主链切到 batch 调用入口
3. 将 batch 提交链改成原子校验、原子提交
4. 将写回校验升级为真正的 batch 校验
5. 更新前端设置文案与进度语义
6. 删除旧的“批次内顺序单块调用”主链与相关冗余封装

## Risks

- `docx` 的 `slot_id` / anchor 稳定性不足，会导致 batch 校验通过后写回对位失败
- `md/tex` 语法 token 提取不完整，会出现“误判可改写”的漏洞
- batch 协议若未把 `rewrite_unit_id` 和顺序约束钉死，仍会出现 suggestion 错绑
- 自动模式若继续把运行中数量按 unit 而不是 batch 展示，前端认知会再次错位

## Validation

必须验证以下硬规则：

- `1 个 batch = 1 次 LLM 调用`
- `units_per_batch = 一次模型调用中包含的 unit 数`
- `max_concurrency = 同时运行的 batch 数`
- `1 个 unit = 1 条 suggestion`
- 可改写 unit 一定能进入同一条安全写回链
- 写回失败时整 batch 失败，不部分成功
- 锁定、语法、占位 slot 绝不被非法修改

测试分组：

1. 协议测试：验证 batch JSON、`batch_id`、result 数量、unit 顺序、slot 冲突
2. LLM 测试：验证 batch 主链一次只调用一次 `call_chat_model`
3. 手动改写测试：验证 `units_per_batch = N` 时一次只构造一个 batch request，成功时生成 N 条 suggestion
4. 自动改写测试：验证 `max_concurrency` 控制 in-flight batch 数，失败时整批 unit 全失败
5. 写回测试：验证 batch 中任一 unit 越界时整批失败，全合法时仍可安全写回
6. 端到端测试：验证“选择 -> batch 调用 -> suggestion -> 审阅 -> 写回”全链一致

## Non-Goals

- 不追求兼容旧 session 数据
- 不保留旧的“批次内顺序单块调用”主链作为 fallback
- 不通过降低写回校验来换取“更多 batch 能过”
- 不把前端 regroup 继续留作正式逻辑

## Result

该设计将当前“批次只是调度分组”的中间态，收敛为一套统一真相：

- 用户分块是真实 suggestion 单元
- batch 是真实模型调用单元
- 写回边界是真实结构单元
- 三者通过结构化 batch 协议和 `SlotUpdate[]` 连接

这样才能同时满足：

- 设置中的“单批处理单元数”与实际调用语义一致
- 自动并发数与 batch 并发语义一致
- 一个 unit 只生成一条结果
- 一个 unit 可严格安全写回
