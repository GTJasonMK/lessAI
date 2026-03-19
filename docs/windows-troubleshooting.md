# Windows 开发运行排错（LessAI / Tauri / pnpm）

这份文档专门应对 Windows 上常见的两类问题：

- `EACCES: permission denied, lstat ... node_modules\\.ignored_*`
- `tauri` / `tauri.cmd` 缺失或“可见但无法运行”（optionalDependencies / native binding）

---

## 推荐运行方式（最重要）

1) **只在 Windows 终端里安装依赖并运行**
- 使用 Windows Terminal（PowerShell 或 CMD）
- 不要在 WSL 里对同一个目录执行 `pnpm install` 后，再回到 Windows 运行

2) **确保项目在 NTFS 磁盘**
- 外接盘/移动盘如果是 exFAT，容易导致链接/权限行为异常
- 检查方式（PowerShell 或 CMD 均可）：

```bat
fsutil fsinfo volumeinfo E: | findstr /i "File System Name"
```

如果看到不是 `NTFS`，建议把项目移到 `C:\Code\LessAI` 这类 NTFS 盘后再安装依赖。

3) **尽量避免杀软拦截 node_modules**
- 如果你装了第三方杀软/策略较严格的 Defender 规则，可能会拦截或锁定某些目录

---

## 问题 1：EACCES / `.ignored_*`（例如 `.ignored_typescript`）

这是 Windows 上最常见的症状之一，通常意味着：

- `node_modules` 是在 **另一个环境** 生成的（例如 WSL / 不同权限用户）
- 或者目录 ACL/权限异常，导致 Node.js 无法 `lstat`/读取
- 或者你在 WSL 开启了 DrvFS metadata，导致某些目录被映射成 Windows 不可读权限（常见于“WSL 装过依赖、Windows 再用”）

### 处理步骤（按顺序执行）

**步骤 A：关闭占用**
- 关闭 IDE/编辑器、终端、以及任何可能占用 `node_modules` 的进程

**步骤 B：删除 node_modules（推荐管理员终端）**

在项目根目录执行：

```bat
rmdir /s /q node_modules
```

如果删除失败（提示拒绝访问/权限不足），再尝试：

```bat
takeown /f node_modules /r /d y
icacls node_modules /grant %USERNAME%:F /t
rmdir /s /q node_modules
```

如果你明确知道 `node_modules` 是在 WSL 里生成的，也可以直接用 WSL 删除（然后只用 Windows 重装）：

```bat
wsl --list --verbose
wsl -d <你的发行版> -- bash -lc "cd /mnt/e/Code/LessAI && rm -rf node_modules"
```

**步骤 C：重新安装（包含 devDependencies）**

```bat
pnpm install --prefer-frozen-lockfile --no-prod
```

---

## 问题 2：`tauri` 找不到 / `Tauri CLI is missing`

### 验证是否安装成功

```bat
pnpm exec tauri --version
```

如果还不行，优先排查：

1) **你是否在 WSL 装过依赖？**（强烈建议按上文删除重装）
2) **optionalDependencies 是否被关闭？**
- 有些机器会设置忽略可选依赖，导致 native binding 缺失

你可以检查（输出如果是 `true` 就不对）：

```bat
pnpm config get ignore-optional
```

如需恢复默认（允许 optional deps）：

```bat
pnpm config set ignore-optional false
```

然后重新安装一次：

```bat
pnpm install --prefer-frozen-lockfile --no-prod
```

---

## 问题 3：出现 `'sh' 不是内部或外部命令`

这通常是 **pnpm 的 script-shell 被配置成 sh**，但当前终端找不到 `sh.exe`。

### 检查

```bat
pnpm config get script-shell
```

如果输出是 `sh`（或类似），建议直接清掉该配置（恢复默认）：

```bat
pnpm config delete script-shell
```

然后重新打开一个新的终端窗口再试。

---

## 一键入口

- 开发运行：双击 `start-lessai.bat`
- 打包：双击 `build-lessai.bat`

这两个脚本内置了依赖自检与修复提示；当依赖异常时会停止并提示，不再“失败但继续跑”。
