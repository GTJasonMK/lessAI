import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const tauriConfigPath = resolve(root, "src-tauri/tauri.conf.json");
const tauriConfig = JSON.parse(readFileSync(tauriConfigPath, "utf8"));

assert.ok(Array.isArray(tauriConfig.bundle?.icon), "bundle.icon 必须为数组");
assert.ok(tauriConfig.bundle.icon.length > 0, "bundle.icon 不能为空");

for (const relativePath of tauriConfig.bundle.icon) {
  const absolutePath = resolve(root, "src-tauri", relativePath);
  assert.ok(existsSync(absolutePath), `打包图标不存在：${relativePath}`);
}

const windowsBundle = tauriConfig.bundle?.windows;
assert.ok(windowsBundle, "必须配置 bundle.windows");

const nsis = windowsBundle?.nsis ?? tauriConfig.bundle?.nsis;
assert.ok(nsis, "必须配置 bundle.nsis");
for (const key of ["installerIcon", "headerImage", "sidebarImage"]) {
  assert.equal(typeof nsis[key], "string", `nsis.${key} 必须为字符串`);
  assert.notEqual(nsis[key].trim(), "", `nsis.${key} 不能为空`);

  const assetPath = resolve(root, "src-tauri", nsis[key]);
  assert.ok(existsSync(assetPath), `NSIS 资源不存在：${nsis[key]}`);
}

const wix = windowsBundle?.wix;
assert.ok(wix, "必须配置 bundle.windows.wix");
for (const key of ["bannerPath", "dialogImagePath"]) {
  assert.equal(typeof wix[key], "string", `wix.${key} 必须为字符串`);
  assert.notEqual(wix[key].trim(), "", `wix.${key} 不能为空`);

  const assetPath = resolve(root, "src-tauri", wix[key]);
  assert.ok(existsSync(assetPath), `WiX 资源不存在：${wix[key]}`);
}

console.log("[packaging-regression] OK");
