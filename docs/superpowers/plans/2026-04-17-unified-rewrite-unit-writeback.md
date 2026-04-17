# Unified Rewrite Batch, Unit, and Safe Writeback Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将当前“批次只是调度分组”的实现重构为真实的 batch 级模型调用，确保 `units_per_batch = 一次模型调用中包含的 unit 数`，同时保持 `1 个 unit = 1 条 suggestion = 同一条安全写回链`。

**Architecture:** 保留 `WritebackSlot` 作为最小安全写回边界，保留 `RewriteUnit` 作为用户可见块与 suggestion 单元，新增 `RewriteBatchRequest/Response` 作为真实调用协议。手动与自动改写主链统一改为 batch 级调用；提交链改为 batch 原子校验、unit 级 suggestion 落地；写回链继续以 slot 真相做安全校验。

**Tech Stack:** Rust, Tauri, reqwest, serde, React, TypeScript, pnpm, cargo test

---

## File Map

- Modify: `src-tauri/src/rewrite_unit/protocol.rs`
  增加 batch 请求/响应协议、解析与校验。
- Modify: `src-tauri/src/rewrite_unit/mod.rs`
  导出 batch 协议类型。
- Modify: `src-tauri/src/rewrite/llm/mod.rs`
  增加 `rewrite_batch_with_client(...)` 与 `rewrite_batch(...)`，让主链真正一次请求处理一个 batch。
- Modify: `src-tauri/src/rewrite/mod.rs`
  导出新的 batch 调用入口。
- Modify: `src-tauri/src/rewrite/llm_regression_tests.rs`
  为 batch 协议和单次模型调用补回归测试。
- Modify: `src-tauri/src/rewrite_jobs/support.rs`
  将“准备批次”从 `Vec<RewriteUnitRequest>` 收口成 `RewriteBatchRequest`。
- Modify: `src-tauri/src/rewrite_jobs/process.rs`
  手动链路改为一次 batch 调用。
- Modify: `src-tauri/src/rewrite_jobs/auto_loop.rs`
  自动链路改为一次 batch 调用。
- Modify: `src-tauri/src/rewrite_jobs/auto_state.rs`
  自动状态链改为接收 batch 响应。
- Modify: `src-tauri/src/rewrite_batch_commit.rs`
  提交入口升级为 batch 原子校验与 unit 级 suggestion 提交。
- Modify: `src-tauri/src/rewrite_writeback.rs`
  写回预校验升级为 batch 级验证。
- Modify: `src-tauri/src/rewrite_jobs_tests.rs`
  补手动批次准备与顺序语义测试。
- Modify: `src-tauri/src/rewrite_writeback_fixture_tests.rs`
  补 batch 写回安全校验测试。
- Modify: `src/components/settings/RewriteStrategyPage.tsx`
  更新设置项语义文案。

## Task 1: 定义 batch 协议并钉死解析规则

**Files:**
- Modify: `src-tauri/src/rewrite_unit/protocol.rs`
- Modify: `src-tauri/src/rewrite_unit/mod.rs`
- Test: `src-tauri/src/rewrite/llm_regression_tests.rs`

- [ ] **Step 1: 写失败测试，证明系统需要 batch 级请求和响应**

```rust
#[test]
fn parse_rewrite_batch_response_rejects_mismatched_batch_id() {
    let request = RewriteBatchRequest::new(
        "batch-1",
        "docx",
        vec![RewriteUnitRequest::new(
            "unit-1",
            "docx",
            vec![RewriteUnitSlot::editable("slot-1", "甲")],
        )],
    );

    let error = parse_rewrite_batch_response(
        &request,
        r#"{"batchId":"batch-x","results":[{"rewriteUnitId":"unit-1","updates":[{"slotId":"slot-1","text":"乙"}]}]}"#,
    )
    .expect_err("expected invalid batch id");

    assert!(error.contains("batchId"));
}
```

- [ ] **Step 2: 运行测试，确认当前缺少 batch 协议而失败**

Run:

```bash
/mnt/c/Windows/System32/cmd.exe /C "cd /d E:\Code\LessAI\src-tauri && cargo test parse_rewrite_batch_response_rejects_mismatched_batch_id -- --exact"
```

Expected: FAIL，报 batch 协议类型或解析函数不存在。

- [ ] **Step 3: 最小实现 batch 协议类型与解析**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RewriteBatchRequest {
    pub batch_id: String,
    pub format: String,
    pub units: Vec<RewriteUnitRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RewriteBatchResponse {
    pub batch_id: String,
    pub results: Vec<RewriteUnitResponse>,
}

pub fn parse_rewrite_batch_response(
    request: &RewriteBatchRequest,
    raw: &str,
) -> Result<RewriteBatchResponse, String> {
    // 校验 batch_id、数量、unit 顺序、slot 权限
}
```

- [ ] **Step 4: 补顺序、重复、数量不一致等失败测试并跑通**

Run:

```bash
/mnt/c/Windows/System32/cmd.exe /C "cd /d E:\Code\LessAI\src-tauri && cargo test rewrite::llm_regression_tests -- --nocapture"
```

Expected: PASS，batch 协议校验测试全部通过。

## Task 2: 将 LLM 主链切为真实 batch 单次调用

**Files:**
- Modify: `src-tauri/src/rewrite/llm/mod.rs`
- Modify: `src-tauri/src/rewrite/mod.rs`
- Test: `src-tauri/src/rewrite/llm_regression_tests.rs`

- [ ] **Step 1: 写失败测试，证明一个 batch 只能触发一次 HTTP 请求**

```rust
#[test]
fn rewrite_batch_with_client_sends_single_http_request() {
    let server = TestServer::start(vec![json_http_response(
        r#"{"batchId":"batch-1","results":[{"rewriteUnitId":"unit-1","updates":[{"slotId":"slot-1","text":"改写1"}]},{"rewriteUnitId":"unit-2","updates":[{"slotId":"slot-2","text":"改写2"}]}]}"#,
    )]);
    let settings = test_settings(&server.base_url);
    let client = build_client(&settings).unwrap();
    let request = RewriteBatchRequest::new(
        "batch-1",
        "docx",
        vec![
            RewriteUnitRequest::new("unit-1", "docx", vec![RewriteUnitSlot::editable("slot-1", "甲")]),
            RewriteUnitRequest::new("unit-2", "docx", vec![RewriteUnitSlot::editable("slot-2", "乙")]),
        ],
    );

    let result = run_async(rewrite_batch_with_client(&client, &settings, &request))
        .expect("batch rewrite should succeed");

    assert_eq!(result.results.len(), 2);
    assert_eq!(server.request_count(), 1);
}
```

- [ ] **Step 2: 运行测试，确认当前实现会缺少 batch 入口而失败**

Run:

```bash
/mnt/c/Windows/System32/cmd.exe /C "cd /d E:\Code\LessAI\src-tauri && cargo test rewrite_batch_with_client_sends_single_http_request -- --exact"
```

Expected: FAIL，报 `rewrite_batch_with_client` 不存在。

- [ ] **Step 3: 实现 batch 调用入口，主链只调用一次 `call_chat_model(...)`**

```rust
pub async fn rewrite_batch_with_client(
    client: &reqwest::Client,
    settings: &AppSettings,
    request: &RewriteBatchRequest,
) -> Result<RewriteBatchResponse, String> {
    let system_prompt = request.system_prompt();
    let user_prompt = request.user_prompt();
    let raw = transport::call_chat_model(client, settings, &system_prompt, &user_prompt, settings.temperature).await?;
    parse_rewrite_batch_response(request, &raw)
}
```

- [ ] **Step 4: 跑 batch LLM 测试，确认单次请求与解析都通过**

Run:

```bash
/mnt/c/Windows/System32/cmd.exe /C "cd /d E:\Code\LessAI\src-tauri && cargo test llm_regression_tests -- --nocapture"
```

Expected: PASS，batch 与 selection 的回归测试都通过。

## Task 3: 切手动与自动改写主链到 batch 调用

**Files:**
- Modify: `src-tauri/src/rewrite_jobs/support.rs`
- Modify: `src-tauri/src/rewrite_jobs/process.rs`
- Modify: `src-tauri/src/rewrite_jobs/auto_loop.rs`
- Modify: `src-tauri/src/rewrite_jobs/auto_state.rs`
- Test: `src-tauri/src/rewrite_jobs_tests.rs`
- Test: `src-tauri/src/rewrite_jobs/auto_loop_tests.rs`

- [ ] **Step 1: 写失败测试，证明手动批次准备必须产出单个 `RewriteBatchRequest`**

```rust
#[test]
fn prepare_loaded_rewrite_batch_builds_single_batch_request() {
    let session = session_with_two_rewrite_units();

    let prepared = prepare_loaded_rewrite_batch(
        &session,
        &["unit-0".to_string(), "unit-1".to_string()],
    )
    .expect("batch should prepare");

    assert_eq!(prepared.batch_request.units.len(), 2);
    assert_eq!(prepared.rewrite_unit_ids, vec!["unit-0".to_string(), "unit-1".to_string()]);
}
```

- [ ] **Step 2: 运行测试，确认当前准备结果不是 batch request 而失败**

Run:

```bash
/mnt/c/Windows/System32/cmd.exe /C "cd /d E:\Code\LessAI\src-tauri && cargo test prepare_loaded_rewrite_batch_builds_single_batch_request -- --exact"
```

Expected: FAIL，报 `batch_request` 字段不存在。

- [ ] **Step 3: 将手动与自动调用都切到 `rewrite_batch(...)`**

```rust
let prepared = prepare_loaded_rewrite_batch(session, rewrite_unit_ids)?;
let completed_batch = commit_rewrite_batch_result(
    app,
    state,
    session_id,
    &prepared.rewrite_unit_ids,
    rewrite::rewrite_batch(&settings, &prepared.batch_request).await,
    batch_commit_mode(auto_approve),
    validate_candidate_batch_writeback,
)?;
```

- [ ] **Step 4: 跑手动/自动主链测试**

Run:

```bash
/mnt/c/Windows/System32/cmd.exe /C "cd /d E:\Code\LessAI\src-tauri && cargo test rewrite_jobs_tests -- --nocapture"
```

Run:

```bash
/mnt/c/Windows/System32/cmd.exe /C "cd /d E:\Code\LessAI\src-tauri && cargo test auto_loop_tests -- --nocapture"
```

Expected: PASS，批次准备、自动批次状态和顺序语义保持正确。

## Task 4: 实现 batch 原子提交与 batch 写回校验

**Files:**
- Modify: `src-tauri/src/rewrite_batch_commit.rs`
- Modify: `src-tauri/src/rewrite_writeback.rs`
- Modify: `src-tauri/src/rewrite_writeback_fixture_tests.rs`

- [ ] **Step 1: 写失败测试，证明 batch 中任一非法 unit 会让整批失败**

```rust
#[test]
fn validate_candidate_batch_writeback_rejects_conflicting_slot_updates_across_units() {
    let session = session_from_docx_fixture();
    let error = validate_candidate_batch_writeback(
        &session,
        &[
            RewriteUnitResponse {
                rewrite_unit_id: "unit-0".to_string(),
                updates: vec![SlotUpdate::new("slot-0", "改写甲")],
            },
            RewriteUnitResponse {
                rewrite_unit_id: "unit-1".to_string(),
                updates: vec![SlotUpdate::new("slot-0", "改写乙")],
            },
        ],
    )
    .expect_err("expected conflicting slot updates to fail");

    assert!(error.contains("slot"));
}
```

- [ ] **Step 2: 运行测试，确认当前 batch 校验没覆盖跨 unit 冲突而失败**

Run:

```bash
/mnt/c/Windows/System32/cmd.exe /C "cd /d E:\Code\LessAI\src-tauri && cargo test validate_candidate_batch_writeback_rejects_conflicting_slot_updates_across_units -- --exact"
```

Expected: FAIL，当前实现未拒绝冲突 slot 更新。

- [ ] **Step 3: 实现 batch 原子校验与 unit 级 suggestion 提交**

```rust
fn normalize_candidate_batch(
    session: &DocumentSession,
    request: &RewriteBatchRequest,
    response: RewriteBatchResponse,
) -> Result<Vec<RewriteUnitResponse>, String> {
    // 校验 batch_id、数量、unit 顺序、slot 冲突，再做文本归一化
}
```

- [ ] **Step 4: 跑 batch 写回与提交测试**

Run:

```bash
/mnt/c/Windows/System32/cmd.exe /C "cd /d E:\Code\LessAI\src-tauri && cargo test rewrite_writeback_fixture_tests -- --nocapture"
```

Expected: PASS，batch 冲突、越界、合法写回样例都通过。

## Task 5: 更新设置语义并做整体验证

**Files:**
- Modify: `src/components/settings/RewriteStrategyPage.tsx`
- Modify: `src/components/SettingsModal.tsx`

- [ ] **Step 1: 更新设置说明文案**

```tsx
<span className="workspace-hint">
  该值表示一次模型调用中最多包含多少个改写单元；它不同于并发数，并发数控制同时运行多少次批量调用。
</span>
```

- [ ] **Step 2: 运行前端类型检查**

Run:

```bash
/mnt/c/Windows/System32/cmd.exe /C "cd /d E:\Code\LessAI && pnpm run typecheck"
```

Expected: PASS。

- [ ] **Step 3: 运行后端核心回归**

Run:

```bash
/mnt/c/Windows/System32/cmd.exe /C "cd /d E:\Code\LessAI\src-tauri && cargo test"
```

Expected: PASS，全部后端测试通过；总时长不超过 60 秒，若超时需中止并排查。
