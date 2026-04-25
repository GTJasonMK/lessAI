import { memo, useEffect, useMemo, useState } from "react";
import { Check, X } from "lucide-react";
import type {
  AppSettings,
  PromptTemplate,
  ProviderCheckResult,
  ReleaseVersionSummary
} from "../lib/types";
import type { NoticeTone } from "../lib/constants";
import { isSettingsReady } from "../lib/helpers";
import { isDemoRuntime } from "../lib/runtimeMode";
import { ActionButton } from "./ActionButton";
import type { ConfirmModalOptions } from "./ConfirmModal";
import { StatusBadge } from "./StatusBadge";
import { PromptSettingsPage } from "./settings/PromptSettingsPage";
import { ProviderSettingsPage } from "./settings/ProviderSettingsPage";
import { RewriteStrategyPage } from "./settings/RewriteStrategyPage";
import { VersionSettingsPage } from "./settings/VersionSettingsPage";

type SettingsPage = "provider" | "version" | "strategy" | "prompt";

interface SettingsModalProps {
  open: boolean;
  settings: AppSettings;
  providerStatus: ProviderCheckResult | null;
  busyAction: string | null;
  /** 切段/标题策略是否锁定（有修改记录时不允许改变） */
  segmentationPresetLocked: boolean;
  /** 锁定原因提示，用于 UI 解释与 title */
  segmentationPresetLockedReason: string;
  onClose: () => void;
  onUpdateStringSetting: <K extends "baseUrl" | "apiKey" | "model" | "updateProxy">(
    key: K,
    value: string
  ) => void;
  onUpdateNumberSetting: (
    key: "timeoutMs" | "temperature" | "maxConcurrency" | "unitsPerBatch",
    value: string
  ) => void;
  onUpdateSegmentationPreset: (value: AppSettings["segmentationPreset"]) => void;
  onUpdateRewriteHeadings: (value: boolean) => void;
  onUpdateRewriteMode: (value: AppSettings["rewriteMode"]) => void;
  onUpdatePromptPresetId: (value: AppSettings["promptPresetId"]) => void;
  onUpsertCustomPrompt: (value: PromptTemplate) => void;
  onDeleteCustomPrompt: (templateId: string) => void;
  currentVersion: string;
  releaseVersions: ReleaseVersionSummary[];
  selectedReleaseTag: string;
  selectedRelease: ReleaseVersionSummary | null;
  selectedReleaseIsCurrent: boolean;
  releaseListLoadedAt: string | null;
  switchRequiresUpdaterManifest: boolean;
  onConfirm: (options: ConfirmModalOptions) => Promise<boolean>;
  onTestProvider: () => void;
  onSaveSettings: () => void;
  onCheckUpdate: () => void;
  onRefreshReleaseVersions: () => void;
  onSelectReleaseTag: (tag: string) => void;
  onSwitchSelectedRelease: () => void;
}

export const SettingsModal = memo(function SettingsModal({
  open,
  settings,
  providerStatus,
  busyAction,
  segmentationPresetLocked,
  segmentationPresetLockedReason,
  onClose,
  onUpdateStringSetting,
  onUpdateNumberSetting,
  onUpdateSegmentationPreset,
  onUpdateRewriteHeadings,
  onUpdateRewriteMode,
  onUpdatePromptPresetId,
  onUpsertCustomPrompt,
  onDeleteCustomPrompt,
  currentVersion,
  releaseVersions,
  selectedReleaseTag,
  selectedRelease,
  selectedReleaseIsCurrent,
  releaseListLoadedAt,
  switchRequiresUpdaterManifest,
  onConfirm,
  onTestProvider,
  onSaveSettings,
  onCheckUpdate,
  onRefreshReleaseVersions,
  onSelectReleaseTag,
  onSwitchSelectedRelease
}: SettingsModalProps) {
  const [page, setPage] = useState<SettingsPage>("provider");
  const demoRuntime = isDemoRuntime();
  const versionSettingsEnabled = !demoRuntime;

  useEffect(() => {
    if (!open) return;
    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        onClose();
      }
    }
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [open, onClose]);

  useEffect(() => {
    if (!open) return;
    // 每次打开设置，默认落在连接配置页，并收起提示词预览，减少干扰。
    setPage("provider");
  }, [open]);

  useEffect(() => {
    if (versionSettingsEnabled) return;
    if (page === "version") {
      setPage("provider");
    }
  }, [page, versionSettingsEnabled]);

  const providerTone: NoticeTone =
    providerStatus == null ? "info" : providerStatus.ok ? "success" : "warning";

  const settingsReady = useMemo(() => isSettingsReady(settings), [settings]);

  if (!open) return null;

  return (
    <div
      className="modal-overlay"
      data-window-drag-exclude="true"
      role="dialog"
      aria-modal="true"
      aria-label="设置"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) {
          onClose();
        }
      }}
    >
      <div className="modal-card">
        <header className="modal-header">
          <div className="modal-header-title">
            <h2>设置</h2>
            <p className="modal-subtitle">
              连接、版本、改写策略、提示词都在这里统一管理
            </p>
          </div>
          <button
            type="button"
            className="icon-button"
            onClick={onClose}
            aria-label="关闭设置"
            title="关闭"
          >
            <X />
          </button>
        </header>

        <div className="modal-body">
          <nav className="settings-nav" aria-label="设置分类">
            <button
              type="button"
              className={`settings-nav-item ${page === "provider" ? "is-active" : ""}`}
              onClick={() => setPage("provider")}
            >
              <strong>模型与接口</strong>
              <span>Base URL / Key / Model</span>
            </button>
            <button
              type="button"
              className={`settings-nav-item ${page === "version" ? "is-active" : ""}`}
              onClick={() => setPage("version")}
              disabled={!versionSettingsEnabled}
              title={
                versionSettingsEnabled
                  ? "检查更新 / 版本切换"
                  : "网页版不支持应用内更新与版本切换"
              }
            >
              <strong>版本管理</strong>
              <span>
                {versionSettingsEnabled
                  ? "检查更新 / 版本切换"
                  : "网页版不可用（仅桌面版）"}
              </span>
            </button>
            <button
              type="button"
              className={`settings-nav-item ${page === "strategy" ? "is-active" : ""}`}
              onClick={() => setPage("strategy")}
            >
              <strong>改写策略</strong>
              <span>切段 / 默认执行模式</span>
            </button>
            <button
              type="button"
              className={`settings-nav-item ${page === "prompt" ? "is-active" : ""}`}
              onClick={() => setPage("prompt")}
            >
              <strong>提示词</strong>
              <span>内置 + 自定义模板</span>
            </button>
          </nav>

          <section className="settings-content" aria-label="设置内容">
            {page === "provider" ? (
              <ProviderSettingsPage
                settings={settings}
                demoRuntime={demoRuntime}
                providerStatus={providerStatus}
                providerTone={providerTone}
                testProviderBusy={busyAction === "test-provider"}
                testProviderDisabled={
                  Boolean(busyAction) && busyAction !== "test-provider"
                }
                onTestProvider={onTestProvider}
                onUpdateStringSetting={onUpdateStringSetting}
                onUpdateNumberSetting={onUpdateNumberSetting}
              />
            ) : null}

            {page === "version" ? (
              <VersionSettingsPage
                currentVersion={currentVersion}
                releaseVersions={releaseVersions}
                selectedReleaseTag={selectedReleaseTag}
                selectedRelease={selectedRelease}
                selectedReleaseIsCurrent={selectedReleaseIsCurrent}
                releaseListLoadedAt={releaseListLoadedAt}
                switchRequiresUpdaterManifest={switchRequiresUpdaterManifest}
                checkUpdateBusy={busyAction === "check-update"}
                checkUpdateDisabled={
                  Boolean(busyAction) && busyAction !== "check-update"
                }
                refreshReleasesBusy={busyAction === "list-releases"}
                refreshReleasesDisabled={
                  Boolean(busyAction) && busyAction !== "list-releases"
                }
                switchReleaseBusy={busyAction === "switch-release-version"}
                switchReleaseDisabled={
                  Boolean(busyAction) && busyAction !== "switch-release-version"
                }
                onCheckUpdate={onCheckUpdate}
                onRefreshReleaseVersions={onRefreshReleaseVersions}
                onSelectReleaseTag={onSelectReleaseTag}
                onSwitchSelectedRelease={onSwitchSelectedRelease}
              />
            ) : null}

            {page === "strategy" ? (
              <RewriteStrategyPage
                settings={settings}
                settingsReady={settingsReady}
                segmentationPresetLocked={segmentationPresetLocked}
                segmentationPresetLockedReason={segmentationPresetLockedReason}
                onUpdateSegmentationPreset={onUpdateSegmentationPreset}
                onUpdateRewriteHeadings={onUpdateRewriteHeadings}
                onUpdateRewriteMode={onUpdateRewriteMode}
                onUpdateNumberSetting={onUpdateNumberSetting}
              />
            ) : null}

            {page === "prompt" ? (
              <PromptSettingsPage
                settings={settings}
                onUpdatePromptPresetId={onUpdatePromptPresetId}
                onUpsertCustomPrompt={onUpsertCustomPrompt}
                onDeleteCustomPrompt={onDeleteCustomPrompt}
                onConfirm={onConfirm}
              />
            ) : null}
          </section>
        </div>

        <footer className="modal-footer">
          <div className="modal-footer-left">
            <StatusBadge tone={settingsReady ? "success" : "warning"}>
              {settingsReady ? "设置已就绪" : "需要配置 Base URL / Key / Model"}
            </StatusBadge>
          </div>

          <div className="modal-footer-actions">
            <ActionButton
              icon={Check}
              label="保存配置"
              busy={busyAction === "save-settings"}
              disabled={Boolean(busyAction) && busyAction !== "save-settings"}
              onClick={onSaveSettings}
              variant="primary"
            />
          </div>
        </footer>
      </div>
    </div>
  );
});
