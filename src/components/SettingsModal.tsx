import { memo, useEffect, useMemo, useState } from "react";
import { ArrowUpCircle, Check, Orbit, Trash2, X } from "lucide-react";
import type { AppSettings, PromptTemplate, ProviderCheckResult } from "../lib/types";
import type { NoticeTone } from "../lib/constants";
import {
  MODE_OPTIONS,
  PRESET_OPTIONS,
  SEGMENTATION_OPTIONS
} from "../lib/constants";
import { PROMPT_PRESETS, makePromptPreview } from "../lib/promptPresets";
import { isSettingsReady } from "../lib/helpers";
import { ActionButton } from "./ActionButton";
import type { ConfirmModalOptions } from "./ConfirmModal";
import { StatusBadge } from "./StatusBadge";

type SettingsPage = "provider" | "strategy" | "prompt";

function makeCustomPromptId() {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return `custom:${crypto.randomUUID()}`;
  }
  return `custom:${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 10)}`;
}

function makeNextCustomPromptName(existing: PromptTemplate[]) {
  const base = "自定义模板";
  const index = existing.length + 1;
  return `${base} ${index}`;
}

interface SettingsModalProps {
  open: boolean;
  settings: AppSettings;
  providerStatus: ProviderCheckResult | null;
  busyAction: string | null;
  onClose: () => void;
  onUpdateStringSetting: <K extends "baseUrl" | "apiKey" | "model" | "updateProxy">(
    key: K,
    value: string
  ) => void;
  onUpdateNumberSetting: (
    key: "timeoutMs" | "temperature" | "maxConcurrency",
    value: string
  ) => void;
  onUpdateChunkPreset: (value: AppSettings["chunkPreset"]) => void;
  onUpdateSegmentationMode: (value: AppSettings["segmentationMode"]) => void;
  onUpdateRewriteMode: (value: AppSettings["rewriteMode"]) => void;
  onUpdatePromptPresetId: (value: AppSettings["promptPresetId"]) => void;
  onUpsertCustomPrompt: (value: PromptTemplate) => void;
  onDeleteCustomPrompt: (templateId: string) => void;
  onConfirm: (options: ConfirmModalOptions) => Promise<boolean>;
  onTestProvider: () => void;
  onSaveSettings: () => void;
  onCheckUpdate: () => void;
}

export const SettingsModal = memo(function SettingsModal({
  open,
  settings,
  providerStatus,
  busyAction,
  onClose,
  onUpdateStringSetting,
  onUpdateNumberSetting,
  onUpdateChunkPreset,
  onUpdateSegmentationMode,
  onUpdateRewriteMode,
  onUpdatePromptPresetId,
  onUpsertCustomPrompt,
  onDeleteCustomPrompt,
  onConfirm,
  onTestProvider,
  onSaveSettings,
  onCheckUpdate
}: SettingsModalProps) {
  const [page, setPage] = useState<SettingsPage>("provider");
  const [showPromptPreview, setShowPromptPreview] = useState(false);

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
    setShowPromptPreview(false);
  }, [open]);

  const providerTone: NoticeTone =
    providerStatus == null ? "info" : providerStatus.ok ? "success" : "warning";

  const settingsReady = useMemo(() => isSettingsReady(settings), [settings]);

  const customPromptPresets = useMemo(() => {
    return settings.customPrompts.map((template) => ({
      id: template.id,
      label: template.name?.trim() ? template.name.trim() : "未命名模板",
      hint: makePromptPreview(template.content, 140) || "（未填写内容）",
      content: template.content
    }));
  }, [settings.customPrompts]);

  const availablePrompts = useMemo(
    () => [...PROMPT_PRESETS, ...customPromptPresets],
    [customPromptPresets]
  );

  const selectedPrompt = useMemo(() => {
    return (
      availablePrompts.find((item) => item.id === settings.promptPresetId) ??
      PROMPT_PRESETS[0]
    );
  }, [availablePrompts, settings.promptPresetId]);

  const selectedCustomPrompt = useMemo(() => {
    return (
      settings.customPrompts.find((item) => item.id === settings.promptPresetId) ?? null
    );
  }, [settings.customPrompts, settings.promptPresetId]);

  const handleAddCustomPrompt = () => {
    const id = makeCustomPromptId();
    const template: PromptTemplate = {
      id,
      name: makeNextCustomPromptName(settings.customPrompts),
      content: selectedPrompt.content.trim()
    };
    onUpsertCustomPrompt(template);
    onUpdatePromptPresetId(id);
    setShowPromptPreview(true);
  };

  const handleDeleteSelectedCustomPrompt = async () => {
    const target = selectedCustomPrompt;
    if (!target) return;

    const ok = await onConfirm({
      title: "删除自定义模板",
      message: `确认删除自定义模板「${target.name || target.id}」？\n\n该操作只会删除模板本身，不会删除文档会话与修改记录。`,
      okLabel: "删除",
      cancelLabel: "取消",
      variant: "danger"
    });

    if (!ok) return;
    onDeleteCustomPrompt(target.id);
  };

  if (!open) return null;

  return (
    <div
      className="modal-overlay"
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
              连接、改写策略、提示词都在这里统一管理
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
              <div className="settings-page">
                <div className="settings-page-head">
                  <h3>模型与接口</h3>
                  <StatusBadge tone={providerTone}>
                    {providerStatus
                      ? providerStatus.ok
                        ? "连接正常"
                        : "待修正"
                      : "未测试"}
                  </StatusBadge>
                </div>

                <div className="field-grid">
                  <label className="field">
                    <span>Base URL</span>
                    <input
                      value={settings.baseUrl}
                      onChange={(event) =>
                        onUpdateStringSetting("baseUrl", event.target.value)
                      }
                      placeholder="https://api.openai.com/v1"
                    />
                  </label>
                  <label className="field">
                    <span>API Key</span>
                    <input
                      type="password"
                      value={settings.apiKey}
                      onChange={(event) =>
                        onUpdateStringSetting("apiKey", event.target.value)
                      }
                      placeholder="sk-..."
                    />
                  </label>
                  <label className="field">
                    <span>Model</span>
                    <input
                      value={settings.model}
                      onChange={(event) =>
                        onUpdateStringSetting("model", event.target.value)
                      }
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
                      onChange={(event) =>
                        onUpdateNumberSetting("timeoutMs", event.target.value)
                      }
                    />
                  </label>
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
                    onChange={(event) =>
                      onUpdateNumberSetting("temperature", event.target.value)
                    }
                  />
                </div>

                <div className="field-block">
                  <div className="field-line">
                    <span>应用更新</span>
                    <strong>网络</strong>
                  </div>
                  <label className="field">
                    <span>更新代理（可选）</span>
                    <input
                      value={settings.updateProxy}
                      onChange={(event) =>
                        onUpdateStringSetting("updateProxy", event.target.value)
                      }
                      placeholder="http://127.0.0.1:7890"
                    />
                  </label>
                  <span className="workspace-hint">留空则直连；仅用于检查/下载更新。</span>
                </div>

                {providerStatus ? (
                  <div className="empty-inline">
                    <span>{providerStatus.message}</span>
                  </div>
                ) : null}
              </div>
            ) : null}

            {page === "strategy" ? (
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
                      {PRESET_OPTIONS.find((item) => item.value === settings.chunkPreset)
                        ?.label}
                    </strong>
                  </div>
                  <div className="segmented-grid">
                    {PRESET_OPTIONS.map((option) => (
                      <button
                        key={option.value}
                        type="button"
                        className={`segment-card ${
                          settings.chunkPreset === option.value ? "is-active" : ""
                        }`}
                        onClick={() => onUpdateChunkPreset(option.value)}
                      >
                        <strong>{option.label}</strong>
                        <span>{option.hint}</span>
                      </button>
                    ))}
                  </div>
                </div>

                <div className="field-block">
                  <div className="field-line">
                    <span>分块模式</span>
                    <strong>
                      {
                        SEGMENTATION_OPTIONS.find(
                          (item) => item.value === settings.segmentationMode
                        )?.label
                      }
                    </strong>
                  </div>
                  <div className="segmented-grid">
                    {SEGMENTATION_OPTIONS.map((option) => (
                      <button
                        key={option.value}
                        type="button"
                        className={`segment-card ${
                          settings.segmentationMode === option.value ? "is-active" : ""
                        }`}
                        onClick={() => onUpdateSegmentationMode(option.value)}
                      >
                        <strong>{option.label}</strong>
                        <span>{option.hint}</span>
                      </button>
                    ))}
                  </div>
                  <span className="workspace-hint">
                    AI 兜底只会在开始执行前，对规则分块质量不足的干净文档做一次重分组；
                    AI 只能返回索引分组，不能返回正文。
                  </span>
                </div>

                <div className="field-block">
                  <div className="field-line">
                    <span>默认执行模式</span>
                    <strong>
                      {MODE_OPTIONS.find((item) => item.value === settings.rewriteMode)
                        ?.label}
                    </strong>
                  </div>
                  <div className="segmented-grid">
                    {MODE_OPTIONS.map((option) => (
                      <button
                        key={option.value}
                        type="button"
                        className={`segment-card ${
                          settings.rewriteMode === option.value ? "is-active" : ""
                        }`}
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
                    onChange={(event) =>
                      onUpdateNumberSetting("maxConcurrency", event.target.value)
                    }
                  />
                  <span className="workspace-hint">
                    并发越高速度越快，但更容易触发接口限速/失败（建议 1–4）。
                  </span>
                </div>
              </div>
            ) : null}

            {page === "prompt" ? (
              <div className="settings-page">
                <div className="settings-page-head">
                  <h3>提示词模板</h3>
                  <StatusBadge tone="info">
                    {PROMPT_PRESETS.length + settings.customPrompts.length} 个模板
                  </StatusBadge>
                </div>

                <span className="workspace-hint">
                  提示：新增/编辑模板后记得点击底部“保存配置”，改写时才会生效。
                </span>

                <div className="field-block">
                  <div className="field-line">
                    <span>内置模板</span>
                    <strong>{PROMPT_PRESETS.length} 个</strong>
                  </div>
                  <div className="prompt-preset-grid">
                    {PROMPT_PRESETS.map((preset) => (
                      <button
                        key={preset.id}
                        type="button"
                        className={`segment-card prompt-preset-card ${
                          settings.promptPresetId === preset.id ? "is-active" : ""
                        }`}
                        onClick={() => onUpdatePromptPresetId(preset.id)}
                      >
                        <strong>{preset.label}</strong>
                        <span>{preset.hint}</span>
                      </button>
                    ))}
                  </div>
                  <span className="workspace-hint">
                    内置模板来自项目中的 prompt/ 文件夹，适合直接选用。
                  </span>
                </div>

                <div className="field-block">
                  <div className="field-line">
                    <span>自定义模板</span>
                    <button
                      type="button"
                      className="switch-chip"
                      onClick={handleAddCustomPrompt}
                      aria-label="新增自定义模板"
                      title="新增自定义模板"
                    >
                      新增模板
                    </button>
                  </div>

                  {customPromptPresets.length ? (
                    <div className="prompt-preset-grid">
                      {customPromptPresets.map((preset) => (
                        <button
                          key={preset.id}
                          type="button"
                          className={`segment-card prompt-preset-card ${
                            settings.promptPresetId === preset.id ? "is-active" : ""
                          }`}
                          onClick={() => onUpdatePromptPresetId(preset.id)}
                        >
                          <strong>{preset.label}</strong>
                          <span>{preset.hint}</span>
                        </button>
                      ))}
                    </div>
                  ) : (
                    <div className="empty-inline">
                      <span>暂无自定义模板。可以点击右上角“新增模板”创建一个。</span>
                    </div>
                  )}

                  <span className="workspace-hint">
                    自定义模板保存在本机 settings.json，不会随文档导出/写回。
                  </span>
                </div>

                {selectedCustomPrompt ? (
                  <div className="field-block">
                    <div className="field-line">
                      <span>编辑自定义模板</span>
                      <button
                        type="button"
                        className="icon-button"
                        onClick={() => void handleDeleteSelectedCustomPrompt()}
                        aria-label="删除当前自定义模板"
                        title="删除当前自定义模板"
                      >
                        <Trash2 />
                      </button>
                    </div>

                    <label className="field">
                      <span>名称</span>
                      <input
                        value={selectedCustomPrompt.name}
                        onChange={(event) =>
                          onUpsertCustomPrompt({
                            ...selectedCustomPrompt,
                            name: event.target.value
                          })
                        }
                        placeholder="例如：中文论文润色（严格保格式）"
                      />
                    </label>

                    <label className="field">
                      <span>内容</span>
                      <textarea
                        className="prompt-preview"
                        value={selectedCustomPrompt.content}
                        onChange={(event) =>
                          onUpsertCustomPrompt({
                            ...selectedCustomPrompt,
                            content: event.target.value
                          })
                        }
                        placeholder="在这里粘贴/编写你的提示词模板…"
                      />
                    </label>
                  </div>
                ) : (
                  <div className="field-block">
                    <div className="assistant-inline-actions">
                      <button
                        type="button"
                        className={`switch-chip ${showPromptPreview ? "is-active" : ""}`}
                        onClick={() => setShowPromptPreview((current) => !current)}
                      >
                        {showPromptPreview ? "收起预览" : "预览当前模板"}
                      </button>
                    </div>

                    {showPromptPreview ? (
                      <label className="field">
                        <span>当前选择：{selectedPrompt.label}</span>
                        <textarea
                          className="prompt-preview"
                          value={selectedPrompt.content.trim()}
                          readOnly
                        />
                      </label>
                    ) : (
                      <div className="empty-inline">
                        <span>
                          当前选择：<strong>{selectedPrompt.label}</strong>
                        </span>
                      </div>
                    )}
                  </div>
                )}
              </div>
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
              icon={ArrowUpCircle}
              label="检查更新"
              busy={busyAction === "check-update"}
              disabled={Boolean(busyAction) && busyAction !== "check-update"}
              onClick={onCheckUpdate}
              variant="secondary"
            />
            <ActionButton
              icon={Orbit}
              label="测试连接"
              busy={busyAction === "test-provider"}
              disabled={
                page !== "provider" ||
                (Boolean(busyAction) && busyAction !== "test-provider")
              }
              onClick={onTestProvider}
              variant="secondary"
              className={page === "provider" ? "" : "is-placeholder"}
            />
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
