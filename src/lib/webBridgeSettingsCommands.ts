import { DEFAULT_SETTINGS } from "./constants";
import { callChatModel, ensureSettingsReady, validateSettings } from "./webBridgeModelApi";
import type { AppSettings, ReleaseVersionSummary } from "./types";

interface SettingsCommandDeps {
  deepClone: <T>(value: T) => T;
  getSettings: () => AppSettings;
  persistSettings: (settings: AppSettings) => void;
  setCachedSettings: (settings: AppSettings) => void;
}

export function createSettingsCommands(deps: SettingsCommandDeps) {
  async function loadSettingsCommand() {
    return deps.deepClone(deps.getSettings());
  }

  async function saveSettingsCommand(settings: AppSettings) {
    const validated = validateSettings({
      ...DEFAULT_SETTINGS,
      ...settings,
      customPrompts: Array.isArray(settings.customPrompts) ? settings.customPrompts : []
    });
    deps.setCachedSettings(deps.deepClone(validated));
    deps.persistSettings(validated);
    return deps.deepClone(validated);
  }

  async function testProviderCommand(settings: AppSettings) {
    const validated = validateSettings({
      ...DEFAULT_SETTINGS,
      ...settings,
      customPrompts: Array.isArray(settings.customPrompts) ? settings.customPrompts : []
    });
    ensureSettingsReady(validated);
    try {
      const text = await callChatModel(
        validated,
        "你是连通性探针。只回复 OK。",
        "OK",
        undefined,
        0
      );
      if (!text) {
        return { ok: false, message: "连接失败：模型返回空文本。" };
      }
      return { ok: true, message: "连接测试通过，chat/completions 可访问。" };
    } catch (error) {
      return {
        ok: false,
        message: `chat/completions 调用失败：${
          error instanceof Error ? error.message : String(error)
        }`
      };
    }
  }

  async function listReleaseVersionsCommand(): Promise<ReleaseVersionSummary[]> {
    return [];
  }

  async function switchReleaseVersionCommand() {
    throw new Error("网页版不支持应用内切换版本。");
  }

  async function installSystemPackageReleaseCommand() {
    throw new Error("网页版不支持系统安装包升级。");
  }

  return {
    loadSettingsCommand,
    saveSettingsCommand,
    testProviderCommand,
    listReleaseVersionsCommand,
    switchReleaseVersionCommand,
    installSystemPackageReleaseCommand
  };
}
