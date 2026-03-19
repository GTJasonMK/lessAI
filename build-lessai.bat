@echo off
setlocal EnableExtensions EnableDelayedExpansion
title LessAI Packager

cd /d "%~dp0"

echo ========================================
echo LessAI Packager
echo ========================================
echo(

where pnpm >nul 2>nul
if errorlevel 1 (
  echo [ERROR] pnpm was not found.
  echo [ERROR] Please install Node.js and pnpm first.
  echo(
  pause
  exit /b 1
)

where cargo >nul 2>nul
if errorlevel 1 (
  echo [ERROR] cargo was not found.
  echo [ERROR] Please install the Rust toolchain first.
  echo(
  pause
  exit /b 1
)

rem 确保不会因为 NODE_ENV=production / 生产模式导致 devDependencies 被跳过
set "NODE_ENV="
set "PNPM_PRODUCTION=false"
set "NPM_CONFIG_PRODUCTION=false"

call :ensure_deps
set "EXIT_CODE=!ERRORLEVEL!"
if not "%EXIT_CODE%"=="0" (
  echo(
  echo [ERROR] Environment check failed with exit code %EXIT_CODE%.
  pause
  exit /b %EXIT_CODE%
)

echo [INFO] Building LessAI (Tauri bundle)...
echo [INFO] This may take a while on first build.
echo(

set "RUST_BACKTRACE=1"
call pnpm exec tauri build
set "EXIT_CODE=%ERRORLEVEL%"

echo(
if not "%EXIT_CODE%"=="0" (
  echo [ERROR] LessAI build failed with exit code %EXIT_CODE%.
  echo [HINT] Make sure you are building in the same OS environment that installed node_modules.
  echo [HINT] If you see optional-deps native binding errors, repair install node_modules.
  echo(
  pause
  exit /b %EXIT_CODE%
)

echo [INFO] Build completed successfully.
echo(
echo [INFO] Output directory (default):
echo   %cd%\\src-tauri\\target\\release\\bundle
echo(
if exist "src-tauri\\target\\release\\bundle" (
  echo [INFO] Bundles:
  dir /b "src-tauri\\target\\release\\bundle"
) else (
  echo [WARN] Bundle directory not found. Tauri output path may differ on your system.
)

echo(
pause
exit /b 0

:ensure_deps
rem 1) 依赖不存在 => 安装（强制包含 devDependencies）
if not exist "node_modules" (
  call :install_deps
  exit /b !ERRORLEVEL!
)

rem 2) tauri.cmd 不存在 => 安装（通常是 devDependencies 没装上）
if not exist "node_modules\\.bin\\tauri.cmd" (
  echo [WARN] Tauri CLI was not found in node_modules.
  call :install_deps
  exit /b !ERRORLEVEL!
)

rem 3) tauri 可执行文件存在，但 native binding 缺失（可选依赖未正确安装）
call pnpm exec tauri --version >nul 2>nul
if not errorlevel 1 (
  exit /b 0
)

echo [WARN] Tauri CLI exists but cannot run (native binding may be missing).
echo [HINT] This often happens when optionalDependencies were not installed correctly.
call :offer_repair_install
exit /b !ERRORLEVEL!

:install_deps
echo [INFO] Installing dependencies (including devDependencies)...
echo [INFO] Command: pnpm install --prefer-frozen-lockfile --no-prod
call pnpm install --prefer-frozen-lockfile --no-prod
if errorlevel 1 (
  echo(
  echo [ERROR] Dependency installation failed.
  call :print_install_hints
  exit /b 1
)

if not exist "node_modules\\.bin\\tauri.cmd" (
  echo(
  echo [ERROR] Tauri CLI is still missing after installation.
  echo [HINT] Run: pnpm install --prefer-frozen-lockfile --no-prod
  echo [HINT] Then verify: pnpm exec tauri --version
  call :print_install_hints
  exit /b 1
)

call pnpm exec tauri --version >nul 2>nul
if errorlevel 1 (
  echo(
  echo [ERROR] Tauri CLI failed to run even though it is installed.
  echo [HINT] This usually means the platform-specific package is missing.
  call :offer_repair_install
  exit /b !ERRORLEVEL!
)

exit /b 0

:offer_repair_install
echo(
echo [INFO] Repair option: remove node_modules and reinstall from scratch.
echo [WARN] This will delete the node_modules directory under the project.
choice /c YN /n /m "Proceed with repair install? (Y/N) "
if errorlevel 2 (
  echo [INFO] Repair install cancelled.
  exit /b 1
)

rmdir /s /q node_modules >nul 2>nul
if errorlevel 1 (
  echo [ERROR] Failed to remove node_modules.
  echo [HINT] Try running this script as Administrator, or close any editors that are using node_modules.
  call :print_install_hints
  exit /b 1
)

call :install_deps
exit /b %ERRORLEVEL%

:print_install_hints
echo [HINT] Common Windows causes:
echo [HINT] - EACCES/EPERM on node_modules\\.ignored_* (e.g. .ignored_typescript) due to filesystem/permissions.
echo [HINT] - Mixed WSL and Windows installs (node_modules created in WSL, then used on Windows).
echo [HINT] - The drive is not NTFS (external drives like exFAT may break symlinks).
echo [HINT] - Windows Developer Mode is off (symlink restrictions) or antivirus blocks node_modules.
echo [HINT] Recommended fix:
echo [HINT] - Open Windows Terminal (not WSL) in this folder.
echo [HINT] - Delete node_modules and reinstall:
echo [HINT]     rmdir /s /q node_modules
echo [HINT]     takeown /f node_modules /r /d y
echo [HINT]     icacls node_modules /grant %USERNAME%:F /t
echo [HINT]     pnpm install --prefer-frozen-lockfile --no-prod
echo [HINT] - Verify:
echo [HINT]     pnpm exec tauri --version
exit /b 0
