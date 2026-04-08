# Selected Chunk Rewrite Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let users multi-select chunks and run manual or auto rewrite only on that subset.

**Architecture:** Keep selection state in the React workbench and pass selected chunk indices into the existing Tauri rewrite entrypoint. Add a small Rust targeting helper so both manual and auto modes share one normalization path and keep concurrency semantics unchanged.

**Tech Stack:** React 19, TypeScript, Tauri 2, Rust

---

### Task 1: Add backend target-subset logic

**Files:**
- Create: `src-tauri/src/rewrite_targets.rs`
- Modify: `src-tauri/src/main.rs`
- Modify: `src-tauri/src/commands/rewrite.rs`
- Modify: `src-tauri/src/rewrite_jobs.rs`
- Test: `src-tauri/src/rewrite_targets.rs`

- [ ] **Step 1: Write failing Rust tests for subset normalization and pending selection**
- [ ] **Step 2: Run `cargo test rewrite_targets -- --nocapture` and verify failure**
- [ ] **Step 3: Implement target normalization, manual next-chunk selection, and auto pending queue helpers**
- [ ] **Step 4: Wire optional `targetChunkIndices` through `start_rewrite` into manual and auto flows**
- [ ] **Step 5: Re-run `cargo test rewrite_targets -- --nocapture` and verify pass**

### Task 2: Add frontend multi-select state and targeting UI

**Files:**
- Create: `src/lib/chunkSelection.ts`
- Modify: `src/App.tsx`
- Modify: `src/app/hooks/useSuggestionActions.ts`
- Modify: `src/app/hooks/useRewriteActions.ts`
- Modify: `src/stages/WorkbenchStage.tsx`
- Modify: `src/stages/workbench/DocumentPanel.tsx`
- Modify: `src/stages/workbench/document/DocumentFlow.tsx`
- Modify: `src/stages/workbench/document/DocumentActionBar.tsx`
- Modify: `src/styles/part-04.css`

- [ ] **Step 1: Add a pure chunk-selection helper for toggling and normalizing selected indices**
- [ ] **Step 2: Thread `selectedChunkIndices` through app state, chunk clicks, and document panel props**
- [ ] **Step 3: Update run-button label/title/disabled logic to switch to `处理所选` when selection exists**
- [ ] **Step 4: Pass selected target indices into the rewrite action hook and preserve selection during active auto jobs**
- [ ] **Step 5: Add visible selected-chunk styling distinct from active-chunk styling**

### Task 3: Verify end-to-end behavior

**Files:**
- Modify: none expected
- Test: `src-tauri/src/rewrite_targets.rs`
- Test: `scripts/ui-regression.test.mjs`

- [ ] **Step 1: Run `cd src-tauri && cargo test rewrite_targets -- --nocapture`**
- [ ] **Step 2: Run `pnpm run typecheck`**
- [ ] **Step 3: Run `node scripts/ui-regression.test.mjs`**
- [ ] **Step 4: Review the diff to confirm only selected-chunk targeting behavior was added**
