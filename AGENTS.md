# Repository Guidelines

## Project Structure & Module Organization
LessAI is a Tauri 2 desktop app with a React/TypeScript UI and a Rust backend.

- `src/`: frontend application (React components, hooks, stages, and CSS).
- `src-tauri/src/`: backend commands, rewrite/session logic, adapters, and Rust tests.
- `scripts/`: Node-based regression checks (UI/scroll/packaging assertions).
- `prompt/`: built-in prompt templates bundled with the app.
- `docs/images/`: documentation screenshots and visual assets.
- `.github/workflows/`: CI and multi-platform bundle/release pipelines.

## Build, Test, and Development Commands
Use `pnpm` (Node version is pinned by `.nvmrc`).

- `pnpm install --frozen-lockfile`: install dependencies deterministically.
- `pnpm run dev`: run Vite frontend only (`0.0.0.0:1420`).
- `pnpm run tauri:dev`: run full desktop app in development.
- `pnpm run typecheck`: TypeScript compile check with no output.
- `pnpm run build`: frontend production build (`tsc && vite build`).
- `pnpm run tauri:build`: build desktop bundles.
- `cd src-tauri && cargo test`: run Rust unit/integration tests.
- `node scripts/ui-regression.test.mjs`: run UI regression assertions.

## Coding Style & Naming Conventions
Follow `.editorconfig` strictly:

- 2 spaces for `ts/tsx/js/jsx/json/css/html/md`.
- 4 spaces for Rust (`*.rs`).
- `LF` line endings for most files; `CRLF` for `*.bat`, `*.cmd`, `*.ps1`.

Naming patterns in this repo:

- React components: `PascalCase` file names (for example, `DocumentPanel.tsx`).
- Hooks: `useXxx` naming (for example, `useRewriteActions.ts`).
- Rust modules/files: `snake_case` (for example, `rewrite_batch_commit.rs`).

## Testing Guidelines
CI currently enforces frontend typecheck and build. Contributors should run:

1. `pnpm run typecheck`
2. `pnpm run build`
3. `cd src-tauri && cargo test`

When changing UI/session/packaging behavior, also run targeted `scripts/*.test.mjs` checks relevant to your change.

## Commit & Pull Request Guidelines
Recent history favors short, imperative commit titles (often Chinese), e.g., `优化docx的分块问题`.

- Keep commits focused to one topic.
- Use clear verb-first summaries; avoid mixed unrelated changes.
- In PRs, include: purpose, key files changed, and validation commands run.
- Link issues when applicable, and attach screenshots for UI-visible updates.
- Ensure CI passes before requesting review.

## Security & Configuration Tips
- API credentials are stored locally by the app (`settings.json` in Tauri app data).
- Never commit local secrets, session artifacts, or machine-specific config dumps.
