import { useCallback } from "react";
import type { AppSettings, DocumentSession, PromptTemplate, ProviderCheckResult } from "../../lib/types";
import { isSettingsReady, readableError } from "../../lib/helpers";
import { saveSettings, testProvider } from "../../lib/api";
import { isDemoRuntime } from "../../lib/runtimeMode";
import type {
  RefreshSessionState,
  ShowNotice,
  WithBusy
} from "./sessionActionShared";

export function useSettingsHandlers(options: {
  settings: AppSettings;
  setSettings: React.Dispatch<React.SetStateAction<AppSettings>>;
  setProviderStatus: React.Dispatch<React.SetStateAction<ProviderCheckResult | null>>;
  currentSession: DocumentSession | null;
  showNotice: ShowNotice;
  withBusy: WithBusy;
  closeSettings: () => void;
  readSegmentationPresetLockedReason: () => string | null;
  refreshSessionState: RefreshSessionState;
}) {
  const {
    settings,
    setSettings,
    setProviderStatus,
    currentSession,
    showNotice,
    withBusy,
    closeSettings,
    readSegmentationPresetLockedReason,
    refreshSessionState
  } = options;

  const handleUpdateStringSetting = useCallback(
    (key: "baseUrl" | "apiKey" | "model" | "updateProxy", value: string) => {
      if (isDemoRuntime() && key === "updateProxy") {
        return;
      }
      if (key !== "updateProxy") {
        setProviderStatus(null);
      }
      setSettings((current) => ({ ...current, [key]: value }));
    },
    [setProviderStatus, setSettings]
  );

  const handleUpdateNumberSetting = useCallback(
    (
      key: "timeoutMs" | "temperature" | "maxConcurrency" | "unitsPerBatch",
      value: string
    ) => {
      const parsed =
        key === "timeoutMs" || key === "maxConcurrency" || key === "unitsPerBatch"
          ? Number.parseInt(value, 10)
          : Number.parseFloat(value);

      if (!Number.isFinite(parsed)) {
        return;
      }

      setProviderStatus(null);
      setSettings((current) => ({
        ...current,
        [key]:
          key === "timeoutMs"
            ? Math.max(1_000, parsed)
            : key === "maxConcurrency"
              ? Math.max(1, Math.min(8, parsed))
              : key === "unitsPerBatch"
                ? Math.max(1, parsed)
              : Math.max(0, Math.min(2, parsed))
      }));
    },
    [setProviderStatus, setSettings]
  );

  const handleUpdateSegmentationPreset = useCallback(
    (value: AppSettings["segmentationPreset"]) => {
      const lockedReason = readSegmentationPresetLockedReason();
      if (lockedReason) {
        showNotice("warning", lockedReason);
        return;
      }
      setProviderStatus(null);
      setSettings((current) => ({ ...current, segmentationPreset: value }));
    },
    [readSegmentationPresetLockedReason, setProviderStatus, setSettings, showNotice]
  );

  const handleUpdateRewriteHeadings = useCallback(
    (value: boolean) => {
      const lockedReason = readSegmentationPresetLockedReason();
      if (lockedReason) {
        showNotice("warning", lockedReason);
        return;
      }
      setProviderStatus(null);
      setSettings((current) => ({ ...current, rewriteHeadings: value }));
    },
    [readSegmentationPresetLockedReason, setProviderStatus, setSettings, showNotice]
  );

  const handleUpdateRewriteMode = useCallback(
    (value: AppSettings["rewriteMode"]) => {
      setProviderStatus(null);
      setSettings((current) => ({ ...current, rewriteMode: value }));
    },
    [setProviderStatus, setSettings]
  );

  const handleUpdatePromptPresetId = useCallback(
    (value: AppSettings["promptPresetId"]) => {
      setSettings((current) => ({ ...current, promptPresetId: value }));
    },
    [setSettings]
  );

  const handleUpsertCustomPrompt = useCallback(
    (template: PromptTemplate) => {
      setSettings((current) => {
        const existingIndex = current.customPrompts.findIndex(
          (item) => item.id === template.id
        );
        const nextPrompts =
          existingIndex >= 0
            ? current.customPrompts.map((item) =>
                item.id === template.id ? template : item
              )
            : [...current.customPrompts, template];

        return { ...current, customPrompts: nextPrompts };
      });
    },
    [setSettings]
  );

  const handleDeleteCustomPrompt = useCallback(
    (templateId: string) => {
      setSettings((current) => {
        const nextPrompts = current.customPrompts.filter(
          (item) => item.id !== templateId
        );
        const nextPresetId =
          current.promptPresetId === templateId
            ? "humanizer_zh"
            : current.promptPresetId;
        return { ...current, customPrompts: nextPrompts, promptPresetId: nextPresetId };
      });
    },
    [setSettings]
  );

  const handleSaveSettings = useCallback(async () => {
    const shouldRefreshCurrentSession =
      !!currentSession &&
      (currentSession.segmentationPreset !== settings.segmentationPreset ||
        currentSession.rewriteHeadings !== settings.rewriteHeadings);

    try {
      const saved = await withBusy("save-settings", () => saveSettings(settings));
      setSettings(saved);

      if (shouldRefreshCurrentSession && currentSession) {
        try {
          await refreshSessionState(currentSession.id, {
            preserveRewriteUnit: false,
            preserveSuggestion: true
          });
          showNotice("success", "配置已保存，当前文档已按新的切段策略刷新。");
        } catch (error) {
          showNotice("error", `配置已保存，但刷新当前文档失败：${readableError(error)}`);
          return;
        }
      } else {
        showNotice("success", "配置已保存，后续打开的文档会沿用当前接口与模型。");
      }

      if (isSettingsReady(saved)) {
        closeSettings();
      }
    } catch (error) {
      showNotice("error", `保存失败：${readableError(error)}`);
    }
  }, [
    closeSettings,
    currentSession,
    refreshSessionState,
    settings,
    setSettings,
    showNotice,
    withBusy
  ]);

  const handleTestProvider = useCallback(async () => {
    try {
      const result = await withBusy("test-provider", () => testProvider(settings));
      setProviderStatus(result);
      showNotice(result.ok ? "success" : "warning", result.message);
    } catch (error) {
      setProviderStatus({ ok: false, message: readableError(error) });
      showNotice("error", `连接测试失败：${readableError(error)}`);
    }
  }, [settings, setProviderStatus, showNotice, withBusy]);

  return {
    handleUpdateStringSetting,
    handleUpdateNumberSetting,
    handleUpdateSegmentationPreset,
    handleUpdateRewriteHeadings,
    handleUpdateRewriteMode,
    handleUpdatePromptPresetId,
    handleUpsertCustomPrompt,
    handleDeleteCustomPrompt,
    handleSaveSettings,
    handleTestProvider
  } as const;
}
