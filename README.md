# LessAI

LessAI 是一个基于 **Tauri 2** 的桌面端中文改写工作台：把“改写”变成可审阅、可回滚、可写回的流程。

你导入文本后，LessAI 会按预设粒度切分为多个片段，调用 **OpenAI 兼容接口**生成改写建议，并以时间线方式展示每条建议的 Diff。你可以逐条应用 / 忽略 / 删除，支持断点续跑，最终导出或一键写回覆盖原文件。

> 本仓库不包含任何模型服务；需要在设置中配置 API Base URL / Key / Model。

---

## 你会用到的核心能力

- 导入：`.txt` / `.md`（单文件）
- 切分粒度：小句 / 整句 / 段落（可配置）
- 生成模式：
  - 手动：一次生成下一段
  - 自动：循环生成（可暂停 / 继续 / 取消）
- 审阅时间线：按顺序保存“修改对”，支持应用 / 忽略 / 删除 / 重试
- 视图：原文 / 改写后 / Diff（含修订标记）
- 持久化：会话 JSON 落盘，支持断点续跑
- Finalize：将已应用结果写回原文件，并清空该文档会话记录

## 使用指南（从 0 到写回）

1. 打开设置，填写：
   - `Base URL`（例如 OpenAI / 兼容中转的地址）
   - `API Key`
   - `Model`（例如 `gpt-4.1-mini`）
2. 打开文件（`.txt`/`.md`）。
3. 选择切分粒度（小句/整句/段落），以及生成模式（手动/自动）。
4. 在右侧时间线审阅每条“修改对”：
   - 应用：纳入最终文本
   - 忽略：跳过但保留记录
   - 删除：从时间线移除
   - 重试：对同一段再次生成
5. 输出：
   - 导出：生成新的文件
   - Finalize：写回覆盖原文件，并清空该文档会话记录

## 下载与运行

推荐直接使用 GitHub Releases 安装包（Windows/macOS/Linux）：

- <https://github.com/GTJasonMK/lessAI/releases>

如果你需要从源码运行/构建，请看下方“开发与构建”。

## 配置与数据存储（重要）

LessAI 会把设置与会话存放在 Tauri 的 `app_data_dir` 目录下（不同系统路径不同）：

- `settings.json`：接口配置与偏好设置
- `sessions/<session_id>.json`：每个文档会话

安全提示：

- `settings.json` 会以明文保存 API Key，请不要把该目录提交到仓库或公开分享。

## Prompt 模板

LessAI 提供两类提示词模板：

- 内置模板：位于 `prompt/`（纯文本），会随应用打包发布；修改后需要重新构建应用才会生效。
- 自定义模板：可在应用设置中新增/编辑，保存在本机 `settings.json`，方便按场景快速切换。

## 开发与构建（给贡献者）

### 技术栈

- Tauri 2（Rust 后端）
- React + TypeScript
- Vite

### 环境要求

- Node.js 20+（仓库提供 `.nvmrc`）
- pnpm 10+
- Rust stable
- 各系统的 Tauri 前置依赖（Windows WebView2、Linux WebKitGTK 等）
  - 参考：<https://v2.tauri.app/start/prerequisites/>

### 本地开发

```bash
pnpm install
pnpm run tauri:dev
```

Windows 也可以直接双击：

- `start-lessai.bat`

### 常用命令

```bash
pnpm run typecheck
pnpm run build
pnpm run tauri:build
```

Rust 单测：

```bash
cd src-tauri
cargo test
```

### 构建产物目录

- `src-tauri/target/release/bundle/`

## 发布（GitHub Actions）

项目采用 tag 触发的 Release 流程：

- 推送 `v*` tag 触发 `.github/workflows/tauri-bundles.yml`
- Workflow 会在 Windows/macOS/Linux 打包，并创建 GitHub Release（包含各平台安装包与校验文件）

```bash
git tag v0.1.0
git push origin v0.1.0
```

## 目录结构（速览）

- `src/`：前端（React/TS）
- `src-tauri/`：后端与打包配置（Rust/Tauri）
- `prompt/`：Prompt 模板
- `.github/workflows/`：CI 与 Release 流程

## License

MIT（见 `LICENSE`）。
