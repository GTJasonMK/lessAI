import { memo } from "react";
import { GitBranch, ListRestart, Orbit } from "lucide-react";
import { formatDate } from "../../lib/helpers";
import type {
  AppSettings,
  ProviderCheckResult,
  ReleaseVersionSummary
} from "../../lib/types";
import type { NoticeTone } from "../../lib/constants";
import { ActionButton } from "../ActionButton";
import { StatusBadge } from "../StatusBadge";

interface ProviderSettingsPageProps {
  settings: AppSettings;
  providerStatus: ProviderCheckResult | null;
  providerTone: NoticeTone;
  testProviderBusy: boolean;
  testProviderDisabled: boolean;
  currentVersion: string;
  releaseVersions: ReleaseVersionSummary[];
  selectedReleaseTag: string;
  selectedRelease: ReleaseVersionSummary | null;
  selectedReleaseIsCurrent: boolean;
  releaseListLoadedAt: string | null;
  refreshReleasesBusy: boolean;
  refreshReleasesDisabled: boolean;
  switchReleaseBusy: boolean;
  switchReleaseDisabled: boolean;
  onTestProvider: () => void;
  onUpdateStringSetting: <K extends "baseUrl" | "apiKey" | "model" | "updateProxy">(
    key: K,
    value: string
  ) => void;
  onUpdateNumberSetting: (
    key: "timeoutMs" | "temperature" | "maxConcurrency" | "unitsPerBatch",
    value: string
  ) => void;
  onRefreshReleaseVersions: () => void;
  onSelectReleaseTag: (tag: string) => void;
  onSwitchSelectedRelease: () => void;
}

export const ProviderSettingsPage = memo(function ProviderSettingsPage({
  settings,
  providerStatus,
  providerTone,
  testProviderBusy,
  testProviderDisabled,
  currentVersion,
  releaseVersions,
  selectedReleaseTag,
  selectedRelease,
  selectedReleaseIsCurrent,
  releaseListLoadedAt,
  refreshReleasesBusy,
  refreshReleasesDisabled,
  switchReleaseBusy,
  switchReleaseDisabled,
  onTestProvider,
  onUpdateStringSetting,
  onUpdateNumberSetting,
  onRefreshReleaseVersions,
  onSelectReleaseTag,
  onSwitchSelectedRelease
}: ProviderSettingsPageProps) {
  return (
    <div className="settings-page">
      <div className="settings-page-head">
        <h3>模型与接口</h3>
        <StatusBadge tone={providerTone}>
          {providerStatus ? (providerStatus.ok ? "连接正常" : "待修正") : "未测试"}
        </StatusBadge>
      </div>

      <div className="field-grid">
        <label className="field">
          <span>Base URL</span>
          <input
            value={settings.baseUrl}
            onChange={(event) => onUpdateStringSetting("baseUrl", event.target.value)}
            placeholder="https://api.openai.com/v1"
          />
        </label>
        <label className="field">
          <span>API Key</span>
          <input
            type="password"
            value={settings.apiKey}
            onChange={(event) => onUpdateStringSetting("apiKey", event.target.value)}
            placeholder="sk-..."
          />
        </label>
        <label className="field">
          <span>Model</span>
          <input
            value={settings.model}
            onChange={(event) => onUpdateStringSetting("model", event.target.value)}
            placeholder="gpt-4.1-mini"
          />
        </label>
        <label className="field field-inline">
          <span>超时（毫秒）</span>
          <input
            type="number"
            min={1000}
            step={1000}
            value={settings.timeoutMs}
            onChange={(event) => onUpdateNumberSetting("timeoutMs", event.target.value)}
          />
        </label>
      </div>

      <div className="settings-page-actions">
        <ActionButton
          icon={Orbit}
          label="测试连接"
          busy={testProviderBusy}
          disabled={testProviderDisabled}
          onClick={onTestProvider}
          variant="secondary"
        />
      </div>

      <div className="field-block">
        <div className="field-line">
          <span>Temperature</span>
          <strong>{settings.temperature.toFixed(1)}</strong>
        </div>
        <input
          type="range"
          min={0}
          max={2}
          step={0.1}
          value={settings.temperature}
          onChange={(event) => onUpdateNumberSetting("temperature", event.target.value)}
        />
      </div>

      <div className="field-block">
        <div className="field-line">
          <span>网络代理</span>
          <strong>网络</strong>
        </div>
        <label className="field">
          <span>代理地址（可选）</span>
          <input
            value={settings.updateProxy}
            onChange={(event) => onUpdateStringSetting("updateProxy", event.target.value)}
            placeholder="http://127.0.0.1:7890"
          />
        </label>
        <span className="workspace-hint">
          留空则直连；用于 AI 模型请求与应用更新（检查/下载）。
        </span>
      </div>

      <div className="field-block">
        <div className="field-line">
          <span>版本管理</span>
          <strong>{currentVersion ? `当前 ${currentVersion}` : "当前版本未知"}</strong>
        </div>
        <label className="field">
          <span>已发布版本</span>
          <select
            value={selectedReleaseTag}
            onChange={(event) => onSelectReleaseTag(event.target.value)}
            disabled={releaseVersions.length === 0}
          >
            {releaseVersions.length === 0 ? (
              <option value="">请先刷新版本列表</option>
            ) : null}
            {releaseVersions.map((release) => (
              <option key={release.tag} value={release.tag}>
                {release.tag}
                {release.prerelease ? "（预发布）" : ""}
                {release.updaterAvailable ? "" : "（仅手动下载）"}
              </option>
            ))}
          </select>
        </label>
        {selectedRelease ? (
          <span className="workspace-hint">
            {selectedRelease.publishedAt
              ? `发布时间：${formatDate(selectedRelease.publishedAt)}`
              : "发布时间未知"}
            {selectedReleaseIsCurrent ? " · 当前正在运行该版本" : ""}
            {!selectedRelease.updaterAvailable
              ? " · 当前版本无 latest.json，需手动下载"
              : ""}
          </span>
        ) : null}
        {releaseListLoadedAt ? (
          <span className="workspace-hint">
            版本列表更新时间：{formatDate(releaseListLoadedAt)}
          </span>
        ) : null}
        <div className="settings-page-actions">
          <ActionButton
            icon={ListRestart}
            label="刷新版本列表"
            busy={refreshReleasesBusy}
            disabled={refreshReleasesDisabled}
            onClick={onRefreshReleaseVersions}
            variant="secondary"
          />
          <ActionButton
            icon={GitBranch}
            label="切换到所选版本"
            busy={switchReleaseBusy}
            disabled={
              switchReleaseDisabled ||
              !selectedRelease ||
              selectedReleaseIsCurrent ||
              !selectedRelease.updaterAvailable
            }
            onClick={onSwitchSelectedRelease}
            variant="secondary"
          />
        </div>
      </div>

      {providerStatus ? (
        <div className="empty-inline">
          <span>{providerStatus.message}</span>
        </div>
      ) : null}
    </div>
  );
});
