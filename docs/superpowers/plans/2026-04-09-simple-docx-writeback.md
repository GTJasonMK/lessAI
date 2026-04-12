# Simple Docx Writeback Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable safe writeback for simple `.docx` files while rejecting unsupported document structures during import.

**Architecture:** Add a strict `SimpleDocxDocument` parser/writer in the Tauri docx adapter, reuse it both for import validation and writeback, and keep the rest of the package untouched. Frontend gates will follow backend capability and stop blocking simple docx editing/finalize flows.

**Tech Stack:** Rust, Tauri, `quick-xml`, `zip`, React, TypeScript

---

### Task 1: Add failing docx adapter tests

**Files:**
- Modify: `src-tauri/src/adapters/docx.rs`

- [ ] **Step 1: Write failing tests for supported simple writeback and unsupported import cases**
- [ ] **Step 2: Run `cmd.exe /c "cd /d E:\\Code\\LessAI\\src-tauri && cargo test docx -- --nocapture"` and confirm failures match missing writeback/validation behavior**

### Task 2: Implement strict simple docx parsing and writeback

**Files:**
- Modify: `src-tauri/src/adapters/docx.rs`
- Modify: `src-tauri/src/documents.rs`
- Modify: `src-tauri/src/commands/export.rs`
- Modify: `src-tauri/src/commands/session.rs`

- [ ] **Step 1: Parse `word/document.xml` into a strict supported model and reject unsupported body/paragraph structures**
- [ ] **Step 2: Reuse the strict parser for import extraction**
- [ ] **Step 3: Add writeback that re-validates the source docx, checks extracted text matches the session/source editor content, rewrites paragraph text, and rebuilds the zip package**
- [ ] **Step 4: Allow `.docx` through existing finalize/editor save flows while keeping `.pdf` blocked**

### Task 3: Open frontend docx writeback paths

**Files:**
- Modify: `src/app/hooks/useDocumentActions.ts`
- Modify: `src/app/hooks/useDocumentFinalizeActions.ts`
- Modify: `src/stages/workbench/DocumentPanel.tsx`

- [ ] **Step 1: Remove docx-only frontend blocks for editor/finalize**
- [ ] **Step 2: Update notices so unsupported docx errors come from backend import instead of misleading frontend warnings**

### Task 4: Verify behavior

**Files:**
- Modify: `scripts/ui-regression.test.mjs` if UI state text changes require it

- [ ] **Step 1: Run targeted Rust tests**
- [ ] **Step 2: Run `pnpm run typecheck`**
- [ ] **Step 3: Run UI regression checks if needed**
