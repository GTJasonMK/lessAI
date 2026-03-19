# LessAI 死代码审计与清理记录

> 目标：全量覆盖仓库 **所有被 Git 跟踪的文件**，定位并移除明确的死代码/无效逻辑，
> 同时保留可追溯的进度记录，避免遗漏与重复。

**审计日期：** 2026-03-19  
**范围口径：** `git ls-files`（共 53 个文件）  

---

## 审计方法（分层）

1) **TypeScript/React（可自动化）**
- `pnpm exec tsc --noUnusedLocals --noUnusedParameters`（额外开启未使用检查）
- 逐文件快速巡检：未引用导出、重复常量、无效分支

2) **Rust/Tauri（受环境限制，偏人工）**
- 当前容器未安装 `cargo`/`rustc`，无法跑 `cargo check/clippy`
- 采用“低风险改动”策略：只做 **无歧义的可见性收敛**、移除明显未引用代码段

3) **脚本/配置/CI**
- 检查未使用脚本、冗余配置、错误的控制流（尤其是 Windows `.bat`）

---

## 覆盖清单（进度勾选）

### 根目录
- [x] `.editorconfig`
- [x] `.gitattributes`
- [x] `.github/workflows/ci.yml`
- [x] `.github/workflows/tauri-bundles.yml`
- [x] `.gitignore`
- [x] `.nvmrc`
- [x] `CLAUDE.md`
- [x] `LICENSE`
- [x] `README.md`
- [x] `build-lessai.bat`
- [x] `index.html`
- [x] `package.json`
- [x] `pnpm-lock.yaml`
- [x] `rust-toolchain.toml`
- [x] `start-lessai.bat`
- [x] `tsconfig.json`
- [x] `tsconfig.node.json`
- [x] `vite.config.ts`

### prompt/
- [x] `prompt/1.txt`
- [x] `prompt/2.txt`

### src-tauri/
- [x] `src-tauri/Cargo.lock`
- [x] `src-tauri/Cargo.toml`
- [x] `src-tauri/build.rs`
- [x] `src-tauri/capabilities/default.json`
- [x] `src-tauri/gen/schemas/acl-manifests.json`
- [x] `src-tauri/gen/schemas/capabilities.json`
- [x] `src-tauri/gen/schemas/desktop-schema.json`
- [x] `src-tauri/gen/schemas/windows-schema.json`
- [x] `src-tauri/icons/icon.ico`
- [x] `src-tauri/icons/icon.png`
- [x] `src-tauri/src/main.rs`
- [x] `src-tauri/src/models.rs`
- [x] `src-tauri/src/rewrite.rs`
- [x] `src-tauri/src/storage.rs`
- [x] `src-tauri/tauri.conf.json`

### src/
- [x] `src/App.tsx`
- [x] `src/main.tsx`
- [x] `src/styles.css`
- [x] `src/vite-env.d.ts`
- [x] `src/components/ActionButton.tsx`
- [x] `src/components/ConfirmModal.tsx`
- [x] `src/components/Panel.tsx`
- [x] `src/components/SettingsModal.tsx`
- [x] `src/components/StatusBadge.tsx`
- [x] `src/hooks/useBusyAction.ts`
- [x] `src/hooks/useNotice.ts`
- [x] `src/hooks/useTauriEvents.ts`
- [x] `src/lib/api.ts`
- [x] `src/lib/constants.ts`
- [x] `src/lib/helpers.ts`
- [x] `src/lib/promptPresets.ts`
- [x] `src/lib/types.ts`
- [x] `src/stages/WorkbenchStage.tsx`

---

## 结论

- **发现的死代码：**
  - 未发现“明确可删除且不会改变行为”的死代码片段（前端/后端核心逻辑均有引用链）。
  - 发现 2 个 Windows 脚本问题会导致“失败但继续执行”，属于控制流 bug（已修复，见下）。

- **已做的清理/验证：**
  - TypeScript：已执行 `tsc --noUnusedLocals --noUnusedParameters`，无未使用声明。
  - Rust：当前容器缺少 `cargo`，未执行 `cargo check/clippy`；仅做结构性阅读排查。
  - Windows 脚本：已修复 `start-lessai.bat` / `build-lessai.bat` 的错误码传播，避免失败后继续启动。

- **需要注意的环境现象（与“死代码”无关，但会影响你本地运行）：**
  - 当前环境下 `pnpm run build` 触发 Rollup 的 Linux native optional 依赖缺失，典型原因是
    `node_modules` 与当前 OS 不一致（例如 Windows 装的依赖拿到 Linux/WSL 用）。

- **已落地的脚本修复点：**
  - 将括号块内的 `exit /b %ERRORLEVEL%` 改为 `exit /b !ERRORLEVEL!`，避免解析时机导致的错误码丢失。
  - 修复后：依赖安装失败 / Tauri CLI 缺失 / Tauri binding 异常时，脚本会立即停止并输出提示。
  - 保持 `.bat` 为 CRLF 行尾，避免 Windows CMD 将整行拆碎导致大量“不是内部或外部命令”报错。
