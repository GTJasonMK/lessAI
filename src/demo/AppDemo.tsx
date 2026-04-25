import { useEffect, useMemo, useRef, useState } from "react";
import { diffTextByLines } from "../lib/diff";

type SegmentationPreset = "clause" | "sentence" | "paragraph";
type SegmentStatus = "idle" | "running" | "done" | "failed";
type SuggestionDecision = "proposed" | "applied" | "dismissed";
type NoticeTone = "info" | "success" | "warning" | "error";

interface DemoSettings {
  baseUrl: string;
  apiKey: string;
  model: string;
  temperature: number;
  segmentationPreset: SegmentationPreset;
}

interface DemoSegment {
  id: string;
  order: number;
  beforeText: string;
  status: SegmentStatus;
  suggestionId: string | null;
  errorMessage: string | null;
}

interface DemoSuggestion {
  id: string;
  segmentId: string;
  beforeText: string;
  afterText: string;
  decision: SuggestionDecision;
}

interface NoticeState {
  tone: NoticeTone;
  message: string;
}

const SETTINGS_STORAGE_KEY = "lessai.demo.settings.v1";

const DEFAULT_SETTINGS: DemoSettings = {
  baseUrl: "",
  apiKey: "",
  model: "gpt-4.1-mini",
  temperature: 0.7,
  segmentationPreset: "sentence"
};

const DEFAULT_SOURCE_TEXT = [
  "在这个快速变化的时代，写作不仅要准确，还要兼顾表达风格与阅读体验。",
  "LessAI Web Demo 仅支持 TXT 演示链路：分段、改写、审阅与导出。",
  "如果你需要 DOCX/TeX/PDF 的安全写回能力，请使用桌面版。"
].join("\n");

function normalizeNewlines(value: string) {
  return value.replace(/\r\n/g, "\n").replace(/\r/g, "\n");
}

function readableError(error: unknown) {
  if (error instanceof Error) {
    return error.message;
  }
  if (typeof error === "string") {
    return error;
  }
  return "发生了未知错误。";
}

function loadDemoSettings() {
  if (typeof window === "undefined") {
    return DEFAULT_SETTINGS;
  }
  try {
    const raw = window.localStorage.getItem(SETTINGS_STORAGE_KEY);
    if (!raw) {
      return DEFAULT_SETTINGS;
    }
    const parsed = JSON.parse(raw) as Partial<DemoSettings>;
    return {
      baseUrl: typeof parsed.baseUrl === "string" ? parsed.baseUrl : DEFAULT_SETTINGS.baseUrl,
      apiKey: typeof parsed.apiKey === "string" ? parsed.apiKey : DEFAULT_SETTINGS.apiKey,
      model: typeof parsed.model === "string" ? parsed.model : DEFAULT_SETTINGS.model,
      temperature:
        typeof parsed.temperature === "number"
          ? Math.min(2, Math.max(0, parsed.temperature))
          : DEFAULT_SETTINGS.temperature,
      segmentationPreset:
        parsed.segmentationPreset === "clause" ||
        parsed.segmentationPreset === "sentence" ||
        parsed.segmentationPreset === "paragraph"
          ? parsed.segmentationPreset
          : DEFAULT_SETTINGS.segmentationPreset
    };
  } catch {
    return DEFAULT_SETTINGS;
  }
}

function persistDemoSettings(settings: DemoSettings) {
  if (typeof window === "undefined") {
    return;
  }
  try {
    window.localStorage.setItem(SETTINGS_STORAGE_KEY, JSON.stringify(settings));
  } catch {
    // ignore storage failures
  }
}

function splitByPattern(text: string, pattern: RegExp) {
  return text
    .split(/\n+/)
    .flatMap((line) => line.match(pattern) ?? [])
    .map((item) => item.trim())
    .filter((item) => item.length > 0);
}

function buildSegments(sourceText: string, preset: SegmentationPreset): DemoSegment[] {
  const normalized = normalizeNewlines(sourceText).trim();
  if (!normalized) {
    return [];
  }

  let pieces: string[] = [];
  if (preset === "paragraph") {
    pieces = normalized
      .split(/\n{2,}/)
      .map((item) => item.trim())
      .filter((item) => item.length > 0);
  } else if (preset === "clause") {
    pieces = splitByPattern(normalized, /[^，,。！？!?；;\n]+[，,。！？!?；;]?/g);
  } else {
    pieces = splitByPattern(normalized, /[^。！？!?；;\n]+[。！？!?；;]?/g);
  }

  if (pieces.length === 0) {
    pieces = [normalized];
  }

  return pieces.map((piece, index) => ({
    id: `segment-${index + 1}`,
    order: index + 1,
    beforeText: piece,
    status: "idle",
    suggestionId: null,
    errorMessage: null
  }));
}

function endpointFromBaseUrl(baseUrl: string) {
  const normalized = baseUrl.trim().replace(/\/+$/, "");
  if (!normalized) {
    throw new Error("请先填写 API Base URL。");
  }
  if (/\/chat\/completions$/i.test(normalized)) {
    return normalized;
  }
  return `${normalized}/chat/completions`;
}

function pickAssistantText(payload: unknown) {
  const response = payload as {
    error?: { message?: unknown };
    choices?: Array<{
      text?: unknown;
      message?: {
        content?: unknown;
      };
    }>;
  };

  if (typeof response.error?.message === "string" && response.error.message.trim()) {
    throw new Error(response.error.message.trim());
  }

  const firstChoice = response.choices?.[0];
  if (!firstChoice) {
    throw new Error("模型没有返回可用结果。");
  }

  const messageContent = firstChoice.message?.content ?? firstChoice.text;
  if (typeof messageContent === "string") {
    const text = messageContent.trim();
    if (!text) {
      throw new Error("模型返回了空文本。");
    }
    return text;
  }

  if (Array.isArray(messageContent)) {
    const text = messageContent
      .map((part) => {
        if (typeof part === "string") {
          return part;
        }
        if (
          typeof part === "object" &&
          part &&
          "text" in part &&
          typeof (part as { text: unknown }).text === "string"
        ) {
          return (part as { text: string }).text;
        }
        return "";
      })
      .join("")
      .trim();
    if (!text) {
      throw new Error("模型返回了空文本。");
    }
    return text;
  }

  throw new Error("模型返回格式不受支持。");
}

async function rewriteWithOpenAICompatible(
  segmentText: string,
  settings: DemoSettings,
  signal?: AbortSignal
) {
  const endpoint = endpointFromBaseUrl(settings.baseUrl);
  const payload = {
    model: settings.model.trim(),
    temperature: settings.temperature,
    messages: [
      {
        role: "system",
        content: "你是中文改写助手。保持原意、优化表达、控制句长。只输出改写后的正文，不要解释。"
      },
      {
        role: "user",
        content: `请改写下面这段中文文本：\n\n${segmentText}`
      }
    ]
  };

  const response = await fetch(endpoint, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${settings.apiKey.trim()}`
    },
    body: JSON.stringify(payload),
    signal
  });

  const rawText = await response.text();
  let parsed: unknown = null;
  try {
    parsed = JSON.parse(rawText);
  } catch {
    parsed = null;
  }

  if (!response.ok) {
    const maybeMessage =
      typeof parsed === "object" &&
      parsed &&
      "error" in parsed &&
      typeof (parsed as { error?: { message?: unknown } }).error?.message === "string"
        ? (parsed as { error: { message: string } }).error.message
        : rawText.slice(0, 200);
    throw new Error(`请求失败（HTTP ${response.status}）：${maybeMessage || "未知错误"}`);
  }

  return pickAssistantText(parsed);
}

function statusLabel(status: SegmentStatus) {
  if (status === "running") return "执行中";
  if (status === "done") return "已完成";
  if (status === "failed") return "失败";
  return "待处理";
}

function decisionLabel(decision: SuggestionDecision) {
  if (decision === "applied") return "已应用";
  if (decision === "dismissed") return "已忽略";
  return "待处理";
}

export default function AppDemo() {
  const [settings, setSettings] = useState<DemoSettings>(() => loadDemoSettings());
  const [sourceText, setSourceText] = useState(DEFAULT_SOURCE_TEXT);
  const [segments, setSegments] = useState<DemoSegment[]>([]);
  const [suggestions, setSuggestions] = useState<DemoSuggestion[]>([]);
  const [selectedSegmentId, setSelectedSegmentId] = useState<string | null>(null);
  const [runningSegmentId, setRunningSegmentId] = useState<string | null>(null);
  const [batchRunning, setBatchRunning] = useState(false);
  const [notice, setNotice] = useState<NoticeState | null>(null);

  const abortControllerRef = useRef<AbortController | null>(null);
  const stopBatchRef = useRef(false);

  useEffect(() => {
    const title = import.meta.env.VITE_DEMO_TITLE?.trim() || "LessAI Web Demo";
    document.title = title;
  }, []);

  useEffect(() => {
    persistDemoSettings(settings);
  }, [settings]);

  const suggestionBySegmentId = useMemo(() => {
    const map = new Map<string, DemoSuggestion>();
    for (const suggestion of suggestions) {
      map.set(suggestion.segmentId, suggestion);
    }
    return map;
  }, [suggestions]);

  const selectedSegment = useMemo(
    () => segments.find((segment) => segment.id === selectedSegmentId) ?? null,
    [segments, selectedSegmentId]
  );

  const selectedSuggestion = useMemo(() => {
    if (!selectedSegment) return null;
    return suggestionBySegmentId.get(selectedSegment.id) ?? null;
  }, [selectedSegment, suggestionBySegmentId]);

  const isSettingsReady = useMemo(() => {
    return (
      settings.baseUrl.trim().length > 0 &&
      settings.apiKey.trim().length > 0 &&
      settings.model.trim().length > 0
    );
  }, [settings]);

  const finalText = useMemo(() => {
    if (segments.length === 0) return "";
    return segments
      .map((segment) => {
        const suggestion = suggestionBySegmentId.get(segment.id);
        if (suggestion?.decision === "applied") {
          return suggestion.afterText;
        }
        return segment.beforeText;
      })
      .join(settings.segmentationPreset === "paragraph" ? "\n\n" : "\n");
  }, [segments, settings.segmentationPreset, suggestionBySegmentId]);

  const diffSpans = useMemo(() => {
    if (!selectedSuggestion) return [];
    return diffTextByLines(selectedSuggestion.beforeText, selectedSuggestion.afterText);
  }, [selectedSuggestion]);

  const appliedCount = useMemo(() => {
    return suggestions.filter((item) => item.decision === "applied").length;
  }, [suggestions]);

  const proposedCount = useMemo(() => {
    return suggestions.filter((item) => item.decision === "proposed").length;
  }, [suggestions]);

  const handlePrepareSegments = () => {
    const builtSegments = buildSegments(sourceText, settings.segmentationPreset);
    if (builtSegments.length === 0) {
      setNotice({ tone: "warning", message: "请输入文本后再开始分段。" });
      return;
    }
    setSegments(builtSegments);
    setSuggestions([]);
    setSelectedSegmentId(builtSegments[0]?.id ?? null);
    setNotice({
      tone: "success",
      message: `已生成 ${builtSegments.length} 个片段，可逐段改写或批量改写。`
    });
  };

  const handleStop = () => {
    stopBatchRef.current = true;
    abortControllerRef.current?.abort();
    setBatchRunning(false);
    setNotice({ tone: "warning", message: "已请求停止当前任务。" });
  };

  const rewriteSingleSegment = async (segmentId: string, fromBatch = false) => {
    if (!isSettingsReady) {
      if (!fromBatch) {
        setNotice({ tone: "warning", message: "请先填写 Base URL、API Key 与 Model。" });
      }
      return false;
    }

    const segment = segments.find((item) => item.id === segmentId);
    if (!segment) {
      return false;
    }

    if (runningSegmentId && runningSegmentId !== segmentId) {
      if (!fromBatch) {
        setNotice({ tone: "warning", message: "已有任务在执行，请稍候。" });
      }
      return false;
    }

    setRunningSegmentId(segmentId);
    setSegments((current) =>
      current.map((item) =>
        item.id === segmentId
          ? {
              ...item,
              status: "running",
              errorMessage: null
            }
          : item
      )
    );

    const controller = new AbortController();
    abortControllerRef.current = controller;

    try {
      const rewritten = await rewriteWithOpenAICompatible(segment.beforeText, settings, controller.signal);
      const suggestion: DemoSuggestion = {
        id: `suggestion-${segmentId}`,
        segmentId,
        beforeText: segment.beforeText,
        afterText: rewritten,
        decision: "proposed"
      };

      setSuggestions((current) => {
        const next = current.filter((item) => item.segmentId !== segmentId);
        next.push(suggestion);
        next.sort((left, right) => left.segmentId.localeCompare(right.segmentId));
        return next;
      });

      setSegments((current) =>
        current.map((item) =>
          item.id === segmentId
            ? {
                ...item,
                status: "done",
                suggestionId: suggestion.id,
                errorMessage: null
              }
            : item
        )
      );

      if (!fromBatch) {
        setSelectedSegmentId(segmentId);
        setNotice({ tone: "success", message: `片段 ${segment.order} 改写完成。` });
      }
      return true;
    } catch (error) {
      if (controller.signal.aborted) {
        setSegments((current) =>
          current.map((item) =>
            item.id === segmentId
              ? {
                  ...item,
                  status: "idle",
                  errorMessage: null
                }
              : item
          )
        );
        return false;
      }

      const message = readableError(error);
      setSegments((current) =>
        current.map((item) =>
          item.id === segmentId
            ? {
                ...item,
                status: "failed",
                errorMessage: message
              }
            : item
        )
      );

      if (!fromBatch) {
        setNotice({ tone: "error", message: `片段 ${segment.order} 改写失败：${message}` });
      }
      return false;
    } finally {
      if (abortControllerRef.current === controller) {
        abortControllerRef.current = null;
      }
      setRunningSegmentId((current) => (current === segmentId ? null : current));
    }
  };

  const handleRewriteAll = async () => {
    if (segments.length === 0) {
      setNotice({ tone: "warning", message: "请先分段后再批量改写。" });
      return;
    }

    if (batchRunning) {
      return;
    }

    stopBatchRef.current = false;
    setBatchRunning(true);
    let successCount = 0;

    for (const segment of segments) {
      if (stopBatchRef.current) {
        break;
      }
      const success = await rewriteSingleSegment(segment.id, true);
      if (success) {
        successCount += 1;
      }
    }

    setBatchRunning(false);
    if (stopBatchRef.current) {
      setNotice({ tone: "warning", message: `任务已停止，已完成 ${successCount}/${segments.length} 个片段。` });
    } else {
      setNotice({ tone: "success", message: `批量改写结束：成功 ${successCount}/${segments.length}。` });
    }
  };

  const handleDecisionChange = (segmentId: string, decision: SuggestionDecision) => {
    setSuggestions((current) =>
      current.map((item) => (item.segmentId === segmentId ? { ...item, decision } : item))
    );
  };

  const handleExport = () => {
    if (!finalText.trim()) {
      setNotice({ tone: "warning", message: "当前没有可导出的文本。" });
      return;
    }
    const blob = new Blob([finalText], { type: "text/plain;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = `lessai-web-demo-${new Date().toISOString().slice(0, 19).replace(/[:T]/g, "-")}.txt`;
    link.click();
    URL.revokeObjectURL(url);
    setNotice({ tone: "success", message: "已导出 TXT 文件。" });
  };

  const handleLoadTxtFile: React.ChangeEventHandler<HTMLInputElement> = async (event) => {
    const file = event.target.files?.[0];
    if (!file) {
      return;
    }
    const isTxt = file.name.toLowerCase().endsWith(".txt") || file.type === "text/plain";
    if (!isTxt) {
      setNotice({ tone: "warning", message: "Web Demo 只支持导入 TXT 文件。" });
      event.target.value = "";
      return;
    }
    const text = await file.text();
    setSourceText(normalizeNewlines(text));
    setSegments([]);
    setSuggestions([]);
    setSelectedSegmentId(null);
    setNotice({ tone: "info", message: `已载入 ${file.name}，请点击“重新分段”。` });
    event.target.value = "";
  };

  const handleResetSession = () => {
    stopBatchRef.current = true;
    abortControllerRef.current?.abort();
    setRunningSegmentId(null);
    setBatchRunning(false);
    setSegments([]);
    setSuggestions([]);
    setSelectedSegmentId(null);
    setNotice({ tone: "info", message: "已清空当前演示会话。" });
  };

  return (
    <div className="demo-page">
      <header className="demo-header">
        <div>
          <p className="demo-kicker">LessAI Web Demo</p>
          <h1>GitHub Pages 演示站（TXT 子集）</h1>
          <p className="demo-subtitle">
            桌面版继续由 Tauri 提供完整能力；网页版仅演示 TXT 的分段改写、审阅与导出。
          </p>
        </div>
        <div className="demo-badge-list">
          <span className="demo-badge">TXT only</span>
          <span className="demo-badge">No Tauri</span>
          <span className="demo-badge">Browser Local</span>
        </div>
      </header>

      <main className="demo-grid">
        <section className="demo-card">
          <h2>1. 模型设置</h2>
          <div className="field-grid">
            <label>
              Base URL
              <input
                value={settings.baseUrl}
                onChange={(event) =>
                  setSettings((current) => ({
                    ...current,
                    baseUrl: event.target.value
                  }))
                }
                placeholder="https://api.openai.com/v1"
              />
            </label>
            <label>
              API Key
              <input
                type="password"
                value={settings.apiKey}
                onChange={(event) =>
                  setSettings((current) => ({
                    ...current,
                    apiKey: event.target.value
                  }))
                }
                placeholder="sk-..."
              />
            </label>
            <label>
              Model
              <input
                value={settings.model}
                onChange={(event) =>
                  setSettings((current) => ({
                    ...current,
                    model: event.target.value
                  }))
                }
                placeholder="gpt-4.1-mini"
              />
            </label>
            <label>
              Temperature ({settings.temperature.toFixed(1)})
              <input
                type="range"
                min={0}
                max={2}
                step={0.1}
                value={settings.temperature}
                onChange={(event) =>
                  setSettings((current) => ({
                    ...current,
                    temperature: Number(event.target.value)
                  }))
                }
              />
            </label>
            <label>
              分段策略
              <select
                value={settings.segmentationPreset}
                onChange={(event) =>
                  setSettings((current) => ({
                    ...current,
                    segmentationPreset: event.target.value as SegmentationPreset
                  }))
                }
              >
                <option value="clause">小句</option>
                <option value="sentence">整句</option>
                <option value="paragraph">段落</option>
              </select>
            </label>
          </div>
          <p className="security-tip">
            提示：API Key 仅存储在当前浏览器 localStorage，用于演示，不会上传到本仓库。
          </p>
        </section>

        <section className="demo-card">
          <h2>2. 输入文本（TXT）</h2>
          <div className="row-actions">
            <label className="file-picker">
              导入 TXT
              <input type="file" accept=".txt,text/plain" onChange={handleLoadTxtFile} />
            </label>
            <button type="button" onClick={handlePrepareSegments}>
              重新分段
            </button>
            <button type="button" className="ghost" onClick={handleResetSession}>
              清空会话
            </button>
          </div>
          <textarea
            className="source-editor"
            value={sourceText}
            onChange={(event) => setSourceText(event.target.value)}
            placeholder="粘贴需要改写的中文文本（支持多段）"
          />
        </section>

        <section className="demo-card">
          <h2>3. 分段与执行</h2>
          <div className="row-actions">
            <button type="button" disabled={!segments.length || batchRunning} onClick={handleRewriteAll}>
              批量改写
            </button>
            <button type="button" className="ghost" disabled={!batchRunning && !runningSegmentId} onClick={handleStop}>
              停止
            </button>
          </div>
          <div className="summary-bar">
            <span>片段：{segments.length}</span>
            <span>建议：{suggestions.length}</span>
            <span>已应用：{appliedCount}</span>
            <span>待处理：{proposedCount}</span>
          </div>
          <ul className="segment-list">
            {segments.map((segment) => {
              const isSelected = segment.id === selectedSegmentId;
              const suggestion = suggestionBySegmentId.get(segment.id);
              return (
                <li
                  key={segment.id}
                  className={isSelected ? "segment-item is-selected" : "segment-item"}
                  onClick={() => setSelectedSegmentId(segment.id)}
                >
                  <div className="segment-meta">
                    <span>#{segment.order}</span>
                    <span className={`status-chip is-${segment.status}`}>{statusLabel(segment.status)}</span>
                    {suggestion ? (
                      <span className={`decision-chip is-${suggestion.decision}`}>
                        {decisionLabel(suggestion.decision)}
                      </span>
                    ) : null}
                  </div>
                  <p>{segment.beforeText}</p>
                  {segment.errorMessage ? <p className="error-message">{segment.errorMessage}</p> : null}
                  <div className="segment-actions">
                    <button
                      type="button"
                      disabled={Boolean(runningSegmentId && runningSegmentId !== segment.id)}
                      onClick={(event) => {
                        event.stopPropagation();
                        void rewriteSingleSegment(segment.id);
                      }}
                    >
                      {suggestion ? "重试改写" : "改写此段"}
                    </button>
                  </div>
                </li>
              );
            })}
          </ul>
        </section>

        <section className="demo-card">
          <h2>4. 审阅与导出</h2>
          {selectedSegment && selectedSuggestion ? (
            <>
              <div className="row-actions">
                <button
                  type="button"
                  className={selectedSuggestion.decision === "applied" ? "is-primary" : ""}
                  onClick={() => handleDecisionChange(selectedSegment.id, "applied")}
                >
                  应用
                </button>
                <button
                  type="button"
                  className={selectedSuggestion.decision === "dismissed" ? "is-primary" : "ghost"}
                  onClick={() => handleDecisionChange(selectedSegment.id, "dismissed")}
                >
                  忽略
                </button>
                <button
                  type="button"
                  className={selectedSuggestion.decision === "proposed" ? "is-primary" : "ghost"}
                  onClick={() => handleDecisionChange(selectedSegment.id, "proposed")}
                >
                  设为待处理
                </button>
              </div>

              <div className="diff-block">
                {diffSpans.map((span, index) => (
                  <span key={`${span.type}-${index}`} className={`diff-span is-${span.type}`}>
                    {span.text}
                  </span>
                ))}
              </div>
            </>
          ) : (
            <p className="empty-hint">请选择已有建议的片段进行审阅。</p>
          )}

          <h3>最终文本预览</h3>
          <textarea className="result-editor" value={finalText} readOnly />
          <div className="row-actions">
            <button type="button" onClick={handleExport}>
              导出 TXT
            </button>
          </div>
        </section>
      </main>

      {notice ? <div className={`notice is-${notice.tone}`}>{notice.message}</div> : null}
    </div>
  );
}
