import { memo } from "react";
import { ArrowUpCircle, GitBranch, ListRestart } from "lucide-react";
import { formatDate } from "../../lib/helpers";
import type { ReleaseVersionSummary } from "../../lib/types";
import { ActionButton } from "../ActionButton";
import { StatusBadge } from "../StatusBadge";

interface VersionSettingsPageProps {
  currentVersion: string;
  releaseVersions: ReleaseVersionSummary[];
  selectedReleaseTag: string;
  selectedRelease: ReleaseVersionSummary | null;
  selectedReleaseIsCurrent: boolean;
  releaseListLoadedAt: string | null;
  switchRequiresUpdaterManifest: boolean;
  checkUpdateBusy: boolean;
  checkUpdateDisabled: boolean;
  refreshReleasesBusy: boolean;
  refreshReleasesDisabled: boolean;
  switchReleaseBusy: boolean;
  switchReleaseDisabled: boolean;
  onCheckUpdate: () => void;
  onRefreshReleaseVersions: () => void;
  onSelectReleaseTag: (tag: string) => void;
  onSwitchSelectedRelease: () => void;
}

export const VersionSettingsPage = memo(function VersionSettingsPage({
  currentVersion,
  releaseVersions,
  selectedReleaseTag,
  selectedRelease,
  selectedReleaseIsCurrent,
  releaseListLoadedAt,
  switchRequiresUpdaterManifest,
  checkUpdateBusy,
  checkUpdateDisabled,
  refreshReleasesBusy,
  refreshReleasesDisabled,
  switchReleaseBusy,
  switchReleaseDisabled,
  onCheckUpdate,
  onRefreshReleaseVersions,
  onSelectReleaseTag,
  onSwitchSelectedRelease
}: VersionSettingsPageProps) {
  return (
    <div className="settings-page">
      <div className="settings-page-head">
        <h3>版本管理</h3>
        <StatusBadge tone={selectedReleaseIsCurrent ? "success" : "info"}>
          {currentVersion ? `当前 ${currentVersion}` : "当前版本未知"}
        </StatusBadge>
      </div>

      <div className="field-block">
        <div className="field-line">
          <span>已发布版本</span>
          <strong>{releaseVersions.length > 0 ? `${releaseVersions.length} 个` : "未加载"}</strong>
        </div>
        <label className="field">
          <span>目标版本</span>
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
        <span className="workspace-hint">
          版本列表刷新 / 检查更新 / 版本切换都会读取“模型与接口”页配置的网络代理。
        </span>
        <div className="settings-page-actions">
          <ActionButton
            icon={ArrowUpCircle}
            label="检查更新"
            busy={checkUpdateBusy}
            disabled={checkUpdateDisabled}
            onClick={onCheckUpdate}
            variant="secondary"
          />
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
              (switchRequiresUpdaterManifest && !selectedRelease.updaterAvailable)
            }
            onClick={onSwitchSelectedRelease}
            variant="secondary"
          />
        </div>
      </div>
    </div>
  );
});
