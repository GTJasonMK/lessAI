# LessAI

LessAI 是一个基于 **Tauri 2 + React + Vite** 的桌面改写工作台：
打开文本文件，按顺序生成“修改对”，在审阅区可追溯地应用/取消，最终导出或写回覆盖原文件。

## 关键能力

- 工作台单屏：**文档全文视图 + 审阅时间线**
- 修改对：按顺序保留（可应用 / 忽略 / 删除），支持断点续跑
- 视图：修改前 / 修改后 / 含修订标记
- Finalize：把已应用结果写回原文件，并清空该文档全部记录

## 开发运行

环境：Node.js 20+（`.nvmrc`）、pnpm 10+、Rust stable（Cargo）。

```bash
pnpm install
pnpm run tauri:dev
```

Windows 也可以直接双击：
- `start-lessai.bat`

建议先用仓库自带的 `test.txt` 走一遍完整流程。

## 打包与发布

本地打包：

```bash
pnpm run tauri:build
```

Windows 也可以直接双击：
- `build-lessai.bat`

全平台打包 + 自动发布 Release：
- 推送 `v*` tag 会触发 GitHub Actions（`.github/workflows/tauri-bundles.yml`），构建 Windows/macOS/Linux 安装包并上传到 GitHub Release。

```bash
git tag v0.1.0
git push origin v0.1.0
```

## 持久化位置

设置与会话保存在系统应用数据目录（Tauri `app_data_dir`）：
- `settings.json`
- `sessions/<session_id>.json`

注意：`settings.json` 会保存 API Key（明文 JSON），不要把该目录上传到仓库或公开分享。

## License

MIT（见 `LICENSE`）。
