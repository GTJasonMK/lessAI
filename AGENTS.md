# Repository Guidelines

## Project Structure & Module Organization
`src/` contains the React + TypeScript desktop UI. Keep app-shell code in `src/app`, reusable UI in `src/components`, shared hooks in `src/hooks`, and helpers, API wrappers, and types in `src/lib`. Main workflow screens live in `src/stages`, and styles are split across `src/styles/part-*.css`. `src-tauri/src/` contains the Rust backend: `commands/` exposes Tauri invokes, `rewrite/` holds rewrite and diff logic, and `adapters/` handles document import formats. Store bundled prompt templates in `prompt/`, screenshots in `docs/images/`, and sample input files in `testdoc/`.

## Build, Test, and Development Commands
Use Node `20` from `.nvmrc`, pnpm `10+`, and stable Rust.

- `pnpm install`: install frontend dependencies.
- `pnpm run tauri:dev`: start the full desktop app with Vite and Tauri.
- `pnpm run typecheck`: run the TypeScript check used in CI.
- `pnpm run build`: compile TypeScript and build the Vite bundle.
- `cd src-tauri && cargo test`: run Rust unit tests, including rewrite coverage.
- `node scripts/ui-regression.test.mjs`: run the lightweight CSS regression check.

## Coding Style & Naming Conventions
Follow `.editorconfig`: 2 spaces for `ts`, `tsx`, `css`, `json`, `html`, and Markdown; 4 spaces for Rust; LF line endings except Windows scripts. Use `PascalCase` for React components, `useCamelCase` for hooks, `camelCase` for helpers, and descriptive file names such as `useRewriteActions.ts` or `rewrite_jobs.rs`. Keep modules focused and move shared types to `src/lib/types.ts` or Rust model modules instead of duplicating shapes.

## Testing Guidelines
Frontend changes should at minimum pass `pnpm run typecheck`. If you touch layout or review-panel styling, also run `node scripts/ui-regression.test.mjs`. Backend changes should add or update tests close to the affected Rust module and pass `cargo test`. There is no dedicated JavaScript unit-test suite yet, so do not merge UI behavior changes without at least typechecking and a manual Tauri smoke test.

## Commit & Pull Request Guidelines
Recent commits use short, imperative Chinese summaries such as `修复编译错误`, `优化ui`, and `添加pdf、tex、doc格式的支持`. Keep the same style: one focused change per commit, leading with a verb. Pull requests should include a concise summary, linked issues when applicable, the commands you ran for verification, and screenshots or GIFs for visible UI changes.

## Security & Configuration Tips
Do not commit real API keys, local `settings.json` data, or release secrets. Signed release bundles depend on GitHub secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`; only use them in release workflows. Treat `prompt/` templates and updater-related files in `src-tauri/` as release-sensitive and coordinate before changing them.
