# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

LessAI is a desktop application for AI-assisted Chinese text rewriting. Users import articles (.txt, .docx, .md, .tex), the app chunks the text into segments, calls an OpenAI-compatible LLM API to rewrite each chunk to sound more naturally human-written, and presents inline diffs for approval/rejection. The goal is to reduce AI-detection scores on existing text.

Built with **Tauri 2** (Rust backend + React 19/TypeScript frontend). Node 20 (see `.nvmrc`).

## Build and Development Commands

```bash
# Install frontend dependencies (pnpm is the package manager)
pnpm install

# Run in development mode (starts both Vite dev server and Tauri window)
pnpm run tauri:dev

# Build production binary
pnpm run tauri:build

# Frontend-only dev server (no Tauri shell, useful for UI iteration)
pnpm dev

# TypeScript type-check
pnpm run typecheck

# Run all Rust tests
cd src-tauri && cargo test

# Run a single Rust test
cd src-tauri && cargo test <test_name>
# e.g.: cargo test normalizes_line_endings_and_blank_lines

# Generate app icons from SVG source
pnpm run icons:generate
```

There is no frontend test framework, no ESLint, no Prettier. Rust tests cover text normalization, segmentation, diff generation (`rewrite.rs`), DOCX extraction (`adapters/docx.rs`), Markdown parsing (`adapters/markdown.rs`), and LaTeX parsing (`adapters/tex.rs`). No tests exist for `main.rs` or `storage.rs`.

## Architecture

### Two-Process Model

Standard Tauri 2 architecture: Rust process hosts the native window and business logic; a webview renders the React UI. All communication flows through Tauri's IPC command system.

```
Frontend (React/TS)                    Backend (Rust)
  src/App.tsx          --invoke-->       src-tauri/src/main.rs (commands + orchestration)
  src/lib/api.ts       <--events--       src-tauri/src/rewrite.rs (LLM + text processing)
  src/lib/types.ts                       src-tauri/src/models.rs (shared types)
  src/lib/diff.ts                        src-tauri/src/storage.rs (JSON file I/O)
                                         src-tauri/src/adapters/ (format adapters)
```

### Backend Modules (src-tauri/src/)

- **main.rs** -- Tauri command handlers (17 commands) and app state. `AppState` tracks running rewrite jobs via `HashMap<String, Arc<JobControl>>` and protects session files with per-session `Mutex` locks. Core workflow: `process_chunk`, `run_manual_rewrite`, `run_auto_loop`. Auto mode runs a queue + in-flight set and respects `max_concurrency` (clamped 1-8).
- **models.rs** -- All shared data types (`AppSettings`, `DocumentSession`, `ChunkTask`, `EditSuggestion`, enums). All structs use `#[serde(rename_all = "camelCase")]` for automatic JS/TS interop.
- **rewrite.rs** -- LLM integration: `rewrite_chunk()` calls an OpenAI-compatible `/chat/completions` endpoint with SSE streaming support. Text processing: `normalize_text()`, `segment_text(text, preset, format, rewrite_headings)` and `segment_regions(regions, preset)` (format-preserving chunking with `skip_rewrite`), `build_diff()` (LCS-based character-level diff).
- **storage.rs** -- JSON file persistence under Tauri's `app_data_dir()`. Sessions stored in `sessions/` subdirectory. Session IDs are UUID v5 derived from file path (same file path = same session, enabling resume).
- **adapters/** -- Format-specific text extraction and region tagging:
  - `docx.rs` -- Extracts text regions from DOCX (reads `word/document.xml` via quick-xml + zip); marks Heading*/Title/Subtitle paragraphs as `skip_rewrite` when `rewriteHeadings=false`
  - `markdown.rs` -- Identifies code blocks, tables, front matter, HTML comments, inline code, links; marks them `skip_rewrite`. Supports blocking headings when `rewriteHeadings=false`
  - `tex.rs` -- Identifies math modes, verbatim/minted/lstlisting environments, comments, commands; marks them `skip_rewrite`. Blocks heading commands (e.g. `\section{}`) when `rewriteHeadings=false`

### Frontend Structure (src/)

State management: pure React `useState`/`useCallback`/`useMemo`/`useRef` -- no external state library. All state lives in `App.tsx` and flows down via props. Uses `startTransition` for large updates.

- **App.tsx** -- Global state center (~1500 lines). Top bar with frameless window controls, settings modal, notifications.
- **stages/WorkbenchStage.tsx** -- Core workbench (~1500 lines). Left: document panel (source/markup/final views + editable source). Right: review timeline (suggestions grouped by chunk, with diff/source/candidate views).
- **lib/api.ts** -- IPC wrapper. 17 functions, each 1:1 mapping to a Tauri command via `invoke()`.
- **lib/types.ts** -- TypeScript interfaces mirroring Rust models (camelCase).
- **lib/diff.ts** -- Frontend diff engine: Myers diff algorithm with two-level comparison (line-level, then character-level refinement within changed blocks). `buildDiffHunks()` produces context-aware hunk output.
- **lib/helpers.ts** -- Utility functions: error handling, settings validation, formatting, text stats, suggestion aggregation, Windows path cleanup (`\\?\` prefix removal).
- **lib/constants.ts** -- UI constants, Tauri event names, default settings, option lists.
- **lib/promptPresets.ts** -- Imports `prompt/1.txt` and `prompt/2.txt` via Vite `?raw` as built-in prompt presets.
- **hooks/useTauriEvents.ts** -- Subscribes to 4 backend events: `rewrite_progress`, `chunk_completed`, `rewrite_finished`, `rewrite_failed`.
- **hooks/useBusyAction.ts** -- Mutual exclusion for async operations (one busy action at a time).
- **components/** -- `SettingsModal.tsx` (3-tab settings: model/strategy/prompts), `ActionButton.tsx`, `Panel.tsx`, `StatusBadge.tsx`, `ConfirmModal.tsx`.
- **styles.css** -- Single CSS file (~2000 lines), no framework.

### Data Flow for Rewriting

1. **Open file**: `open_document(path)` -> backend loads settings + detects format (.txt/.docx/.md/.tex) -> adapter processes text/regions (and `rewriteHeadings`) -> `normalize_text()` -> `segment_text()` / `segment_regions()` -> create/resume `DocumentSession` (JSON persisted)
2. **Start rewrite**: `start_rewrite(session_id, mode)` -> backend spawns async task -> manual mode: one chunk at a time; auto mode: runs up to `max_concurrency` chunks concurrently -> each chunk: LLM API call (SSE streaming) -> `build_diff()` -> create `EditSuggestion` -> emit `chunk_completed` event
3. **Review**: user applies/dismisses/deletes suggestions, or retries a chunk
4. **Export**: `export_document` merges applied suggestions into output file
5. **Finalize**: `finalize_document` writes merged result back to original file and deletes session JSON

### IPC Commands (Tauri Commands)

Settings: `load_settings`, `save_settings`, `test_provider`
Sessions: `open_document`, `load_session`, `reset_session`
Rewrite: `start_rewrite`, `pause_rewrite`, `resume_rewrite`, `cancel_rewrite`, `retry_chunk`
Suggestions: `apply_suggestion`, `dismiss_suggestion`, `delete_suggestion`
Document: `save_document_edits`, `export_document`, `finalize_document`

### Backend Events (Rust -> Frontend)

- `rewrite_progress` -- Progress update (completed count, running chunk indices)
- `chunk_completed` -- Single chunk finished with suggestion
- `rewrite_finished` -- All chunks done
- `rewrite_failed` -- Error occurred

## Design System

Bauhaus-inspired (see `src/styles.css` for source of truth):

- Color palette: paper (#f5efe4), ink (#141414), red (#d62d20), blue (#1744cf), yellow (#efc122), green (#1f7a3c)
- Hard offset box shadows (never blurred), thick 3px black borders, large border radii (24px/18px/12px)
- Typography: Newsreader (serif) for headings, Roboto (sans-serif) for body
- Icons: lucide-react
- Frameless window with custom title bar (`-webkit-app-region: drag`)
- Diff highlighting: insert = green background, delete = red background + strikethrough

## Key Conventions

- All user-facing strings are in **Simplified Chinese**
- Rust structs use `snake_case` with `#[serde(rename_all = "camelCase")]` for automatic JS/TS interop
- Prompt presets: `prompt/1.txt` (aigc_v1) and `prompt/2.txt` (humanizer_zh) are used by both backend (`include_str!`) and frontend (`?raw` import). `prompt/3.txt` exists but is not referenced in code.
- The app connects to any OpenAI-compatible API (configurable base URL, API key, model name)
- Session persistence is file-based JSON (no database), one JSON file per document session
- CI: GitHub Actions runs typecheck on all pushes; `tauri-bundles.yml` builds cross-platform releases on version tags
