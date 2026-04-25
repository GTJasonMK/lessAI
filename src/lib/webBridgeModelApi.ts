import type { AppSettings } from "./types";

export function ensureSettingsReady(settings: AppSettings) {
  if (!settings.baseUrl.trim()) {
    throw new Error("Base URL 不能为空。");
  }
  if (!settings.apiKey.trim()) {
    throw new Error("API Key 不能为空。");
  }
  if (!settings.model.trim()) {
    throw new Error("模型名称不能为空。");
  }
}

export function validateSettings(settings: AppSettings): AppSettings {
  if (settings.timeoutMs < 1_000) {
    throw new Error("超时（毫秒）必须大于等于 1000。");
  }
  if (settings.maxConcurrency < 1 || settings.maxConcurrency > 8) {
    throw new Error("自动并发数必须在 1 到 8 之间。");
  }
  if (settings.unitsPerBatch < 1) {
    throw new Error("单批处理单元数必须大于等于 1。");
  }
  if (settings.temperature < 0 || settings.temperature > 2) {
    throw new Error("Temperature 必须在 0 到 2 之间。");
  }
  return settings;
}

function chatUrl(baseUrl: string) {
  const normalized = baseUrl.trim().replace(/\/+$/g, "");
  if (normalized.endsWith("/chat/completions")) {
    return normalized;
  }
  if (normalized.endsWith("/v1")) {
    return `${normalized}/chat/completions`;
  }
  return `${normalized}/v1/chat/completions`;
}

function extractChatContent(payload: unknown) {
  const data = payload as {
    error?: { message?: unknown };
    choices?: Array<{ text?: unknown; message?: { content?: unknown } }>;
  };
  if (typeof data.error?.message === "string" && data.error.message.trim()) {
    throw new Error(data.error.message.trim());
  }
  const choice = data.choices?.[0];
  if (!choice) {
    throw new Error("模型没有返回可用结果。");
  }
  const content = choice.message?.content ?? choice.text;
  if (typeof content === "string") {
    const trimmed = content.trim();
    if (!trimmed) {
      throw new Error("模型返回内容为空。");
    }
    return trimmed;
  }
  if (Array.isArray(content)) {
    const merged = content
      .map((item) => {
        if (typeof item === "string") return item;
        if (
          typeof item === "object" &&
          item &&
          "text" in item &&
          typeof (item as { text: unknown }).text === "string"
        ) {
          return (item as { text: string }).text;
        }
        return "";
      })
      .join("")
      .trim();
    if (!merged) {
      throw new Error("模型返回内容为空。");
    }
    return merged;
  }
  throw new Error("模型返回格式不受支持。");
}

export async function callChatModel(
  settings: AppSettings,
  systemPrompt: string,
  userPrompt: string,
  signal?: AbortSignal,
  temperatureOverride?: number
) {
  const response = await fetch(chatUrl(settings.baseUrl), {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${settings.apiKey.trim()}`
    },
    body: JSON.stringify({
      model: settings.model.trim(),
      temperature:
        typeof temperatureOverride === "number"
          ? temperatureOverride
          : settings.temperature,
      messages: [
        { role: "system", content: systemPrompt },
        { role: "user", content: userPrompt }
      ]
    }),
    signal
  });

  const raw = await response.text();
  let json: unknown = null;
  try {
    json = JSON.parse(raw);
  } catch {
    json = null;
  }

  if (!response.ok) {
    const message =
      typeof json === "object" &&
      json &&
      "error" in json &&
      typeof (json as { error?: { message?: unknown } }).error?.message === "string"
        ? (json as { error: { message: string } }).error.message
        : raw.slice(0, 200);
    throw new Error(`模型调用失败：HTTP ${response.status} ${message}`);
  }

  return extractChatContent(json);
}

export function sanitizeFileName(name: string) {
  const cleaned = name.trim().replace(/[<>:"/\\|?*\u0000-\u001f]/g, "_");
  return cleaned.length > 0 ? cleaned : "lessai-result";
}
