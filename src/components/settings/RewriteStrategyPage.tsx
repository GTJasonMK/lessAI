import { memo } from "react";
import type { AppSettings } from "../../lib/types";
import { MODE_OPTIONS, PRESET_OPTIONS } from "../../lib/constants";
import { StatusBadge } from "../StatusBadge";

interface RewriteStrategyPageProps {
  settings: AppSettings;
  settingsReady: boolean;
  segmentationPresetLocked: boolean;
  segmentationPresetLockedReason: string;
  onUpdateSegmentationPreset: (value: AppSettings["segmentationPreset"]) => void;
  onUpdateRewriteHeadings: (value: boolean) => void;
  onUpdateRewriteMode: (value: AppSettings["rewriteMode"]) => void;
  onUpdateNumberSetting: (
    key: "timeoutMs" | "temperature" | "maxConcurrency" | "unitsPerBatch",
    value: string
  ) => void;
}

export const RewriteStrategyPage = memo(function RewriteStrategyPage({
  settings,
  settingsReady,
  segmentationPresetLocked,
  segmentationPresetLockedReason,
  onUpdateSegmentationPreset,
  onUpdateRewriteHeadings,
  onUpdateRewriteMode,
  onUpdateNumberSetting
}: RewriteStrategyPageProps) {
  return (
    <div className="settings-page">
      <div className="settings-page-head">
        <h3>改写策略</h3>
        <StatusBadge tone={settingsReady ? "success" : "warning"}>
          {settingsReady ? "可执行" : "未配置"}
        </StatusBadge>
      </div>

      <div className="field-block">
        <div className="field-line">
          <span>默认切段策略</span>
          <strong>
            {PRESET_OPTIONS.find((item) => item.value === settings.segmentationPreset)?.label}
          </strong>
        </div>
        <div className="segmented-grid">
          {PRESET_OPTIONS.map((option) => (
            <button
              key={option.value}
              type="button"
              className={`segment-card ${settings.segmentationPreset === option.value ? "is-active" : ""}`}
              onClick={() => onUpdateSegmentationPreset(option.value)}
              disabled={segmentationPresetLocked}
              title={segmentationPresetLocked ? segmentationPresetLockedReason : option.hint}
            >
              <strong>{option.label}</strong>
              <span>{option.hint}</span>
            </button>
          ))}
        </div>
        {segmentationPresetLocked ? (
          <span className="workspace-hint">{segmentationPresetLockedReason}</span>
        ) : (
          <span className="workspace-hint">
            提示：切段策略属于“项目级配置”。项目产生修改对/进度后会锁定；如需调整，请先重置记录或打开新文档。
          </span>
        )}
      </div>

      <div className="field-block">
        <div className="field-line">
          <span>标题/章节是否允许改写</span>
          <strong>{settings.rewriteHeadings ? "允许" : "屏蔽"}</strong>
        </div>
        <div className="segmented-grid">
          <button
            type="button"
            className={`segment-card ${!settings.rewriteHeadings ? "is-active" : ""}`}
            onClick={() => onUpdateRewriteHeadings(false)}
            disabled={segmentationPresetLocked}
            title={segmentationPresetLocked ? segmentationPresetLockedReason : ""}
          >
            <strong>默认屏蔽</strong>
            <span>导入时标记标题为不可改写</span>
          </button>
          <button
            type="button"
            className={`segment-card ${settings.rewriteHeadings ? "is-active" : ""}`}
            onClick={() => onUpdateRewriteHeadings(true)}
            disabled={segmentationPresetLocked}
            title={segmentationPresetLocked ? segmentationPresetLockedReason : ""}
          >
            <strong>允许改写</strong>
            <span>标题也参与降重（更激进）</span>
          </button>
        </div>
        {segmentationPresetLocked ? (
          <span className="workspace-hint">{segmentationPresetLockedReason}</span>
        ) : (
          <span className="workspace-hint">
            提示：该开关只影响“新导入/重置后”的切块；已生成会话需重置记录才生效。
          </span>
        )}
      </div>

      <div className="field-block">
        <div className="field-line">
          <span>默认执行模式</span>
          <strong>
            {MODE_OPTIONS.find((item) => item.value === settings.rewriteMode)?.label}
          </strong>
        </div>
        <div className="segmented-grid">
          {MODE_OPTIONS.map((option) => (
            <button
              key={option.value}
              type="button"
              className={`segment-card ${settings.rewriteMode === option.value ? "is-active" : ""}`}
              onClick={() => onUpdateRewriteMode(option.value)}
            >
              <strong>{option.label}</strong>
              <span>{option.hint}</span>
            </button>
          ))}
        </div>
      </div>

      <div className="field-block">
        <div className="field-line">
          <span>自动并发数</span>
          <strong>{settings.maxConcurrency}</strong>
        </div>
        <input
          type="range"
          min={1}
          max={8}
          step={1}
          value={settings.maxConcurrency}
          onChange={(event) => onUpdateNumberSetting("maxConcurrency", event.target.value)}
        />
        <span className="workspace-hint">
          并发越高速度越快，但更容易触发接口限速/失败（建议 1–4）。
        </span>
      </div>

      <div className="field-block">
        <div className="field-line">
          <span>单批处理单元数</span>
          <strong>{settings.unitsPerBatch}</strong>
        </div>
        <input
          type="number"
          min={1}
          step={1}
          value={settings.unitsPerBatch}
          onChange={(event) => onUpdateNumberSetting("unitsPerBatch", event.target.value)}
        />
        <span className="workspace-hint">
          该值表示一次模型调用中最多包含多少个改写单元；它不同于并发数，并发数控制同时运行多少次批量调用。
        </span>
      </div>
    </div>
  );
});
