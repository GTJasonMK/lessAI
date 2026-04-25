import type { DocumentSession, SlotUpdate, WritebackSlot } from "./types";
import { mergedTextFromSlots } from "./webBridgeSessionUtils";

function randomId(prefix: string) {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return `${prefix}-${crypto.randomUUID()}`;
  }
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 9)}`;
}
export const WEB_DOCUMENT_FORMAT = "plainText";
const UNIT_RESPONSE_FORMAT =
  "{\"rewriteUnitId\":\"...\",\"updates\":[{\"slotId\":\"...\",\"text\":\"...\"}]}";
const BATCH_RESPONSE_FORMAT =
  "{\"batchId\":\"...\",\"results\":[{\"rewriteUnitId\":\"...\",\"updates\":[{\"slotId\":\"...\",\"text\":\"...\"}]}]}";
export const REWRITE_UNIT_NOT_FOUND_ERROR = "改写单元不存在。";
const WRITEBACK_SLOT_NOT_FOUND_ERROR = "未找到对应的写回槽位。";

export interface RewriteUnitSlotPayload {
  slotId: string;
  text: string;
  separatorAfter: string;
  editable: boolean;
  role: WritebackSlot["role"];
}

export interface RewriteUnitRequestPayload {
  rewriteUnitId: string;
  format: string;
  displayText: string;
  slots: RewriteUnitSlotPayload[];
}

export interface RewriteBatchRequestPayload {
  batchId: string;
  format: string;
  units: RewriteUnitRequestPayload[];
}

export interface RawSlotUpdatePayload {
  slotId: string;
  text: string;
}

export interface RawRewriteUnitResponsePayload {
  rewriteUnitId: string;
  updates: RawSlotUpdatePayload[];
}

export interface RawRewriteBatchResponsePayload {
  batchId: string;
  results: RawRewriteUnitResponsePayload[];
}

export interface RewriteUnitResponsePayload {
  rewriteUnitId: string;
  updates: SlotUpdate[];
}

export interface RewriteBatchResponsePayload {
  batchId: string;
  results: RewriteUnitResponsePayload[];
}

export function rewriteUnitSystemPrompt() {
  return (
    "你是文档改写助手。请只返回 JSON，格式必须为 " +
    `${UNIT_RESPONSE_FORMAT}。` +
    "只能改写 editable=true 的 slot；locked slot 只能用于理解上下文，不能出现在 updates 中。" +
    "slot 中的 separatorAfter 表示原文换行/段落边界，只能用于理解结构，不能试图改动它。" +
    "必须原样返回 rewriteUnitId，不要输出解释或 Markdown 代码块。"
  );
}

export function rewriteBatchSystemPrompt() {
  return (
    "你是文档批量改写助手。请只返回 JSON，格式必须为 " +
    `${BATCH_RESPONSE_FORMAT}。` +
    "必须原样返回 batchId。results 的顺序必须与输入 units 顺序完全一致。" +
    "每个结果只能改写本 unit 中 editable=true 的 slot；locked slot 只能用于理解上下文，不能出现在 updates 中。" +
    "slot 中的 separatorAfter 表示原文换行/段落边界，只能用于理解结构，不能试图改动它。" +
    "不要输出解释、注释或 Markdown 代码块。"
  );
}

export function rewriteUnitUserPrompt(request: RewriteUnitRequestPayload) {
  return `请基于以下改写单元生成 JSON 结果：\n${JSON.stringify(request, null, 2)}`;
}

export function rewriteBatchUserPrompt(request: RewriteBatchRequestPayload) {
  return `请基于以下改写批次生成 JSON 结果：\n${JSON.stringify(request, null, 2)}`;
}

export function parseRawJsonObject(raw: string, errorPrefix: string) {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch (error) {
    throw new Error(
      `${errorPrefix}：${error instanceof Error ? error.message : String(error)}`
    );
  }

  if (typeof parsed !== "object" || !parsed || Array.isArray(parsed)) {
    throw new Error(`${errorPrefix}：返回值不是 JSON 对象。`);
  }
  return parsed as Record<string, unknown>;
}

export function parseRawSlotUpdates(value: unknown, errorPrefix: string): RawSlotUpdatePayload[] {
  if (value == null) {
    return [];
  }
  if (!Array.isArray(value)) {
    throw new Error(`${errorPrefix}：updates 字段不是数组。`);
  }

  return value.map((item, index) => {
    if (typeof item !== "object" || !item || Array.isArray(item)) {
      throw new Error(`${errorPrefix}：updates[${index}] 不是对象。`);
    }
    const slotId = (item as Record<string, unknown>).slotId;
    const text = (item as Record<string, unknown>).text;
    if (typeof slotId !== "string" || !slotId.trim()) {
      throw new Error(`${errorPrefix}：updates[${index}].slotId 缺失或不是字符串。`);
    }
    if (typeof text !== "string") {
      throw new Error(`${errorPrefix}：updates[${index}].text 缺失或不是字符串。`);
    }
    return { slotId, text };
  });
}

export function parseRawRewriteUnitResponse(raw: string): RawRewriteUnitResponsePayload {
  const parsed = parseRawJsonObject(raw, "改写单元返回不是合法 JSON");
  const rewriteUnitId = parsed.rewriteUnitId;
  if (typeof rewriteUnitId !== "string" || !rewriteUnitId.trim()) {
    throw new Error("改写单元返回不是合法 JSON：rewriteUnitId 缺失或不是字符串。");
  }
  return {
    rewriteUnitId,
    updates: parseRawSlotUpdates(parsed.updates, "改写单元返回不是合法 JSON")
  };
}

export function parseRawRewriteBatchResponse(raw: string): RawRewriteBatchResponsePayload {
  const parsed = parseRawJsonObject(raw, "改写批次返回不是合法 JSON");
  const batchId = parsed.batchId;
  if (typeof batchId !== "string" || !batchId.trim()) {
    throw new Error("改写批次返回不是合法 JSON：batchId 缺失或不是字符串。");
  }
  const rawResults = parsed.results;
  if (rawResults == null) {
    return { batchId, results: [] };
  }
  if (!Array.isArray(rawResults)) {
    throw new Error("改写批次返回不是合法 JSON：results 字段不是数组。");
  }
  const results = rawResults.map((item, index) => {
    if (typeof item !== "object" || !item || Array.isArray(item)) {
      throw new Error(`改写批次返回不是合法 JSON：results[${index}] 不是对象。`);
    }
    const rewriteUnitId = (item as Record<string, unknown>).rewriteUnitId;
    if (typeof rewriteUnitId !== "string" || !rewriteUnitId.trim()) {
      throw new Error(
        `改写批次返回不是合法 JSON：results[${index}].rewriteUnitId 缺失或不是字符串。`
      );
    }
    return {
      rewriteUnitId,
      updates: parseRawSlotUpdates(
        (item as Record<string, unknown>).updates,
        "改写批次返回不是合法 JSON"
      )
    };
  });
  return { batchId, results };
}

export function validateSingleSlotUpdate(
  slotPermissions: Map<string, boolean>,
  seen: Set<string>,
  update: RawSlotUpdatePayload
) {
  if (!slotPermissions.has(update.slotId)) {
    throw new Error(`未知 slot_id：${update.slotId}。`);
  }
  if (!slotPermissions.get(update.slotId)) {
    throw new Error(`locked slot 不允许修改：${update.slotId}。`);
  }
  if (seen.has(update.slotId)) {
    throw new Error(`slot_id 重复：${update.slotId}。`);
  }
  seen.add(update.slotId);
}

export function validateSlotUpdates(request: RewriteUnitRequestPayload, updates: RawSlotUpdatePayload[]) {
  const slotPermissions = new Map<string, boolean>();
  for (const slot of request.slots) {
    slotPermissions.set(slot.slotId, slot.editable);
  }

  const seen = new Set<string>();
  for (const update of updates) {
    validateSingleSlotUpdate(slotPermissions, seen, update);
  }
}

export function parseRewriteUnitResponse(
  request: RewriteUnitRequestPayload,
  raw: string
): RewriteUnitResponsePayload {
  const parsed = parseRawRewriteUnitResponse(raw);
  if (parsed.rewriteUnitId !== request.rewriteUnitId) {
    throw new Error(
      `rewriteUnitId 不匹配：期望 ${request.rewriteUnitId}，实际 ${parsed.rewriteUnitId}。`
    );
  }
  validateSlotUpdates(request, parsed.updates);
  return {
    rewriteUnitId: parsed.rewriteUnitId,
    updates: parsed.updates.map((update) => ({ slotId: update.slotId, text: update.text }))
  };
}

export function parseRewriteBatchResponse(
  request: RewriteBatchRequestPayload,
  raw: string
): RewriteBatchResponsePayload {
  const parsed = parseRawRewriteBatchResponse(raw);
  if (parsed.batchId !== request.batchId) {
    throw new Error(`batchId 不匹配：期望 ${request.batchId}，实际 ${parsed.batchId}。`);
  }
  if (parsed.results.length !== request.units.length) {
    throw new Error(
      `results 数量不匹配：期望 ${request.units.length}，实际 ${parsed.results.length}。`
    );
  }

  const results: RewriteUnitResponsePayload[] = [];
  for (let index = 0; index < request.units.length; index += 1) {
    const unitRequest = request.units[index];
    const result = parsed.results[index];
    if (result.rewriteUnitId !== unitRequest.rewriteUnitId) {
      throw new Error(
        `rewriteUnitId 不匹配：期望 ${unitRequest.rewriteUnitId}，实际 ${result.rewriteUnitId}。`
      );
    }
    validateSlotUpdates(unitRequest, result.updates);
    results.push({
      rewriteUnitId: result.rewriteUnitId,
      updates: result.updates.map((update) => ({
        slotId: update.slotId,
        text: update.text
      }))
    });
  }

  return {
    batchId: parsed.batchId,
    results
  };
}

export function findWritebackSlot(session: DocumentSession, slotId: string) {
  const slot = session.writebackSlots.find((item) => item.id === slotId);
  if (!slot) {
    throw new Error(WRITEBACK_SLOT_NOT_FOUND_ERROR);
  }
  return slot;
}

export function rewriteUnitSlots(session: DocumentSession, rewriteUnitId: string) {
  const rewriteUnit = session.rewriteUnits.find((item) => item.id === rewriteUnitId);
  if (!rewriteUnit) {
    throw new Error(REWRITE_UNIT_NOT_FOUND_ERROR);
  }
  return rewriteUnit.slotIds.map((slotId) => findWritebackSlot(session, slotId));
}

export function buildRewriteUnitRequestFromSlots(
  rewriteUnitId: string,
  slots: WritebackSlot[],
  format: string
): RewriteUnitRequestPayload {
  return {
    rewriteUnitId,
    format,
    displayText: mergedTextFromSlots(slots),
    slots: slots.map((slot) => ({
      slotId: slot.id,
      text: slot.text,
      separatorAfter: slot.separatorAfter,
      editable: slot.editable,
      role: slot.role
    }))
  };
}

export function buildRewriteUnitRequest(session: DocumentSession, rewriteUnitId: string) {
  const slots = rewriteUnitSlots(session, rewriteUnitId);
  return buildRewriteUnitRequestFromSlots(rewriteUnitId, slots, WEB_DOCUMENT_FORMAT);
}

export function buildRewriteBatchRequest(units: RewriteUnitRequestPayload[]) {
  const format = units[0]?.format;
  if (!format) {
    throw new Error("改写批次不包含任何单元。");
  }
  return {
    batchId: randomId("batch"),
    format,
    units
  };
}
