import { memo, useMemo, useState } from "react";
import { Trash2 } from "lucide-react";
import type { AppSettings, PromptTemplate } from "../../lib/types";
import { PROMPT_PRESETS, makePromptPreview } from "../../lib/promptPresets";
import type { ConfirmModalOptions } from "../ConfirmModal";
import { StatusBadge } from "../StatusBadge";

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

interface PromptSettingsPageProps {
  settings: AppSettings;
  onUpdatePromptPresetId: (value: AppSettings["promptPresetId"]) => void;
  onUpsertCustomPrompt: (value: PromptTemplate) => void;
  onDeleteCustomPrompt: (templateId: string) => void;
  onConfirm: (options: ConfirmModalOptions) => Promise<boolean>;
}

export const PromptSettingsPage = memo(function PromptSettingsPage({
  settings,
  onUpdatePromptPresetId,
  onUpsertCustomPrompt,
  onDeleteCustomPrompt,
  onConfirm
}: PromptSettingsPageProps) {
  const [showPromptPreview, setShowPromptPreview] = useState(false);

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

  return (
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
              className={`segment-card prompt-preset-card ${settings.promptPresetId === preset.id ? "is-active" : ""}`}
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
                className={`segment-card prompt-preset-card ${settings.promptPresetId === preset.id ? "is-active" : ""}`}
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
  );
});

