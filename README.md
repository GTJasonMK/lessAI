<p align="center">
  <img src="src-tauri/icons/lessai-logo.svg" width="180" alt="LessAI Logo" />
</p>

<h1 align="center">LessAI</h1>

<p align="center">
  AI 辅助中文改写工作台 · 可审阅 · 可回滚 · 可写回
</p>

<p align="center">
  <a href="https://github.com/GTJasonMK/lessAI/releases">Releases</a>
  ·
  <a href="https://gtjasonmk.github.io/lessAI/">Web Demo</a>
  ·
  <a href="https://github.com/GTJasonMK/lessAI/issues">Issues</a>
  ·
  <a href="LICENSE">License</a>
</p>

<p align="center">
  <img alt="CI" src="https://github.com/GTJasonMK/lessAI/actions/workflows/ci.yml/badge.svg" />
  <img alt="Bundles" src="https://github.com/GTJasonMK/lessAI/actions/workflows/tauri-bundles.yml/badge.svg" />
</p>

---

## ✨ 简介

LessAI 是一个基于 **Tauri 2** 的桌面端中文改写工作台：把“改写”变成可审阅、可回滚、可写回的流程。

你导入文本后，LessAI 会按预设粒度切分为多个片段，调用 **OpenAI 兼容接口**生成改写建议，并以时间线方式展示每条建议的 Diff。你可以逐条应用 / 忽略 / 删除，支持断点续跑，最终导出或一键写回覆盖原文件。

> DOCX 兼容策略是“安全优先”而不是“尽力乱写回”：常见复杂结构会被保留为锁定占位符继续导入；遇到无法确认安全写回边界的结构时，应用会明确阻断，而不是冒险改坏原文档。

> 本仓库不包含任何模型服务；需要在设置中配置 API Base URL / Key / Model。

## ✅ 你会用到的核心能力

- 导入：`.txt` / `.md` / `.tex` / `.docx`
- 切分粒度：小句 / 整句 / 段落（可配置）
- 生成模式：
  - 手动：一次生成下一段
  - 自动：循环生成（可暂停 / 继续 / 取消）
- 审阅时间线：按顺序保存“修改对”，支持应用 / 忽略 / 删除 / 重试
- 视图：原文 / 改写后 / Diff（含修订标记）
- 持久化：会话 JSON 落盘，支持断点续跑
- Finalize：将已应用结果写回原文件，并清空该文档会话记录
- DOCX 处理：尽量保留编号、超链接、公式/分页符/表格等结构；复杂对象默认锁定，不参与 AI 改写

## 🧭 界面导览

<table>
  <tr>
    <th align="center">工作台（文档 + 审阅时间线）</th>
    <th align="center">设置（策略 + 模板）</th>
  </tr>
  <tr>
    <td align="center">
      <img src="docs/images/workbench.png" width="460" alt="LessAI 工作台（Workbench）" />
    </td>
    <td align="center">
      <img src="docs/images/settings.png" width="460" alt="LessAI 设置（Settings）" />
    </td>
  </tr>
</table>

### 工作台（Workbench）

- 顶部工具栏（只说和“断点续跑/写回”相关的）
  - `打开文件`：同一文件再次打开会自动恢复该文件的会话进度（修改对、已应用状态、待生成片段）
  - `导出`：导出当前终稿到新文件，**不清理记录**；建议在“覆盖写回原文件”前先导出做备份
- 文档视图切换：`修订标记 / 修改后 / 修改前`
  - `修订标记`：把“插入/删除”直接标在全文上，审阅时能一眼看到改动密度与范围
  - `修改后`：按当前选择，把**已应用**的修改合并成整篇终稿（便于通读把关）
  - `修改前`：始终展示原文整篇，方便对照
- 重置会话记录（不改原文件）：文档区右上角“回转箭头”
  - 清空该文档的修改对/进度，并按当前切段策略重新切块
  - 适合“切段策略/提示词改了，想从头再跑一遍”，但又不想动原文件
- 覆盖写回原文件并清理记录：文档区右上角“文件打勾”
  - 只把【已应用】的修改写回到原文件
  - 写回成功后删除该文档的全部历史记录（修改对、进度），并自动以“全新会话”重新打开
- 批处理控制：`开始优化/开始批处理` + `暂停/继续/取消`
  - `人工把关` 模式：一次生成下一段，生成后停下，等待你在右侧审阅
  - `自动批处理` 模式：后台连续生成，并把结果**自动应用**到时间线（你仍可随时撤销/删除）
- 审阅视图切换：`Diff / 原文 / 候选稿`
  - `Diff`：展示插入/删除片段（最适合“快速决策”）
  - `原文`：展示该条修改对对应的改写前文本（适合对齐语义）
  - `候选稿`：展示该条修改对生成的改写后文本（适合通读质量）
- 审阅区的“时间线”不是一次性结果：每条修改对都带序号，可追溯
  - `应用`：对当前片段生效；同一片段若已有已应用版本，会自动切换到最新选择，保证终稿确定性
  - `忽略/撤销`：将该修改对标记为不采用；若该条已应用，相当于撤销，终稿会回退到原文（或其他仍在待审阅的候选）
  - `删除`：从时间线移除该条修改对；若该片段不再有任何修改对，会回到“待生成”状态，便于重新生成
- 失败可重试：某个片段生成失败时，可以只对该片段重试，不影响其他片段与既有审阅结果

### 设置（Settings）

- 左侧三类配置入口（不是摆设，都会影响实际行为）
  - `模型与接口`：支持 OpenAI 兼容接口（Base URL / Key / Model），并可在页面内测试连通性
  - `改写策略`：决定“怎么切段、默认怎么跑”
  - `提示词`：内置模板 + 可编辑的自定义模板（用于不同写作场景快速切换）
- 默认切段策略：`小句 / 整句 / 段落`
  - 影响生成粒度、速度与成本；粒度越细越可控，但轮次更多
  - 已打开的文档若想应用新的切段策略，可回到工作台使用“重置会话记录”重新切块
- 默认执行模式：`人工把关 / 自动批处理`
  - 决定工作台主按钮的默认行为（手动逐段 vs 后台连续）
- 自动并发数：控制自动批处理的并发，提升吞吐，但并发越高越容易触发限速/失败（建议 1–4）

## 📊 效果展示（朱雀 AI 检测对比）

以下为示例截图，仅代表当时测试内容、提示词与检测模型下的结果，**不构成任何保证**。
检测工具与评分规则可能随时间变化，请以你的实际场景测试为准。

### 偏理测试（技术类）

<table>
  <tr>
    <th align="center">使用前</th>
    <th align="center">使用后</th>
  </tr>
  <tr>
    <td align="center">
      <img src="docs/images/zhuque-tech-before.png" width="420" alt="偏理测试：使用前（朱雀 AI 检测）" />
    </td>
    <td align="center">
      <img src="docs/images/zhuque-tech-after.png" width="420" alt="偏理测试：使用后（朱雀 AI 检测）" />
    </td>
  </tr>
</table>

### 偏文测试（随笔类）

<table>
  <tr>
    <th align="center">使用前</th>
    <th align="center">使用后</th>
  </tr>
  <tr>
    <td align="center">
      <img src="docs/images/zhuque-essay-before.png" width="420" alt="偏文测试：使用前（朱雀 AI 检测）" />
    </td>
    <td align="center">
      <img src="docs/images/zhuque-essay-after.png" width="420" alt="偏文测试：使用后（朱雀 AI 检测）" />
    </td>
  </tr>
</table>

## 🚀 使用指南（从 0 到写回）

1. 打开设置，填写：
   - `Base URL`（例如 OpenAI / 兼容中转的地址）
   - `API Key`
   - `Model`（例如 `gpt-4.1-mini`）
2. 打开文件（`.txt` / `.md` / `.tex` / `.docx`）。
3. 选择切分粒度（小句/整句/段落），以及生成模式（手动/自动）。
4. 在右侧时间线审阅每条“修改对”：
   - 应用：纳入最终文本
   - 忽略：跳过但保留记录
   - 删除：从时间线移除
   - 重试：对同一段再次生成
5. 输出：
   - 导出：生成新的文件
   - Finalize：写回覆盖原文件，并清空该文档会话记录

## 📦 下载与运行

推荐直接使用 GitHub Releases 安装包（Windows/macOS/Linux）：

- <https://github.com/GTJasonMK/lessAI/releases>

Web 演示版（GitHub Pages）：

- <https://gtjasonmk.github.io/lessAI/>

> Pages 演示站仅提供 TXT 子集能力（分段改写/审阅/导出），不包含桌面版的 DOCX/TeX/PDF 安全写回与 Tauri 会话能力。

如果你需要从源码运行/构建，请看下方“开发与构建”。

## 🔐 配置与数据存储（重要）

LessAI 会把设置与会话存放在 Tauri 的 `app_data_dir` 目录下（不同系统路径不同）：

- `settings.json`：接口配置与偏好设置
- `sessions/<session_id>.json`：每个文档会话

安全提示：

- `settings.json` 会以明文保存 API Key，请不要把该目录提交到仓库或公开分享。

## 🧩 Prompt 模板

LessAI 提供两类提示词模板：

- 内置模板：位于 `prompt/`（纯文本），会随应用打包发布；修改后需要重新构建应用才会生效。
- 自定义模板：可在应用设置中新增/编辑，保存在本机 `settings.json`，方便按场景快速切换。

## 🛠️ 开发与构建（给贡献者）

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

Linux 也可使用仓库内脚本（会自动检查/修复依赖）：

```bash
chmod +x start-lessai.sh build-lessai.sh scripts/lessai-linux-common.sh
./start-lessai.sh
```

### 常用命令

```bash
pnpm run typecheck
pnpm run build
pnpm run build:demo
pnpm run dev:demo
pnpm run tauri:build
```

Linux 打包（脚本封装）：

```bash
./build-lessai.sh
```

Rust 单测：

```bash
cd src-tauri
cargo test
```

完整测试清单见 [docs/testing-guide.md](docs/testing-guide.md)。

### 构建产物目录

- `src-tauri/target/release/bundle/`

## 🏷️ 发布（GitHub Actions）

项目采用 tag 触发的 Release 流程：

- 推送 `v*` tag 触发 `.github/workflows/tauri-bundles.yml`
- Workflow 会在 Windows/macOS/Linux 打包，并创建 GitHub Release（包含各平台安装包与校验文件）
- 每个 Release 说明都会固定包含文档兼容边界提示：DOCX/PDF 走“安全优先”写回策略，复杂结构可能导入可读但拒绝写回
- `master` 分支 push 会触发 `.github/workflows/pages-demo.yml`，自动发布 TXT 演示站到 GitHub Pages

```bash
git tag v0.1.0
git push origin v0.1.0
```

## 🗂️ 目录结构（速览）

- `src/`：前端（React/TS）
- `src-tauri/`：后端与打包配置（Rust/Tauri）
- `prompt/`：Prompt 模板
- `.github/workflows/`：CI 与 Release 流程

## 🙏 致谢

- 感谢 **[Linuxdo](https://linux.do/)** 社区的交流、分享与反馈，让 LessAI 的迭代更高效。

## 📄 License

MIT（见 `LICENSE`）。
