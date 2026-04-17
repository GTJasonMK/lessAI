use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::{SlotUpdate, WritebackSlotRole};

const UNIT_RESPONSE_FORMAT: &str =
    r#"{"rewriteUnitId":"...","updates":[{"slotId":"...","text":"..."}]}"#;
const BATCH_RESPONSE_FORMAT: &str =
    r#"{"batchId":"...","results":[{"rewriteUnitId":"...","updates":[{"slotId":"...","text":"..."}]}]}"#;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RewriteUnitSlot {
    pub slot_id: String,
    pub text: String,
    pub editable: bool,
    pub role: WritebackSlotRole,
}

impl RewriteUnitSlot {
    #[cfg(test)]
    pub fn editable(slot_id: &str, text: &str) -> Self {
        Self {
            slot_id: slot_id.to_string(),
            text: text.to_string(),
            editable: true,
            role: WritebackSlotRole::EditableText,
        }
    }

    #[cfg(test)]
    pub fn locked(slot_id: &str, text: &str, role: WritebackSlotRole) -> Self {
        Self {
            slot_id: slot_id.to_string(),
            text: text.to_string(),
            editable: false,
            role,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RewriteUnitRequest {
    pub rewrite_unit_id: String,
    pub format: String,
    pub display_text: String,
    pub slots: Vec<RewriteUnitSlot>,
}

impl RewriteUnitRequest {
    pub fn new(rewrite_unit_id: &str, format: &str, slots: Vec<RewriteUnitSlot>) -> Self {
        Self {
            rewrite_unit_id: rewrite_unit_id.to_string(),
            format: format.to_string(),
            display_text: slots.iter().map(|slot| slot.text.as_str()).collect(),
            slots,
        }
    }

    pub fn system_prompt(&self) -> String {
        format!(
            "你是文档改写助手。请只返回 JSON，格式必须为 {UNIT_RESPONSE_FORMAT}。\
             只能改写 editable=true 的 slot；locked slot 只能用于理解上下文，不能出现在 updates 中。\
             必须原样返回 rewriteUnitId，不要输出解释或 Markdown 代码块。"
        )
    }

    pub fn user_prompt(&self) -> String {
        let payload =
            serde_json::to_string_pretty(self).expect("rewrite unit request should serialize");
        format!("请基于以下改写单元生成 JSON 结果：\n{payload}")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RewriteBatchRequest {
    pub batch_id: String,
    pub format: String,
    pub units: Vec<RewriteUnitRequest>,
}

impl RewriteBatchRequest {
    pub fn new(batch_id: &str, format: &str, units: Vec<RewriteUnitRequest>) -> Self {
        Self {
            batch_id: batch_id.to_string(),
            format: format.to_string(),
            units,
        }
    }

    pub fn system_prompt(&self) -> String {
        format!(
            "你是文档批量改写助手。请只返回 JSON，格式必须为 {BATCH_RESPONSE_FORMAT}。\
             必须原样返回 batchId。results 的顺序必须与输入 units 顺序完全一致。\
             每个结果只能改写本 unit 中 editable=true 的 slot；locked slot 只能用于理解上下文，不能出现在 updates 中。\
             不要输出解释、注释或 Markdown 代码块。"
        )
    }

    pub fn user_prompt(&self) -> String {
        let payload =
            serde_json::to_string_pretty(self).expect("rewrite batch request should serialize");
        format!("请基于以下改写批次生成 JSON 结果：\n{payload}")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RewriteUnitResponse {
    pub rewrite_unit_id: String,
    pub updates: Vec<SlotUpdate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RewriteBatchResponse {
    pub batch_id: String,
    pub results: Vec<RewriteUnitResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawRewriteUnitResponse {
    rewrite_unit_id: String,
    #[serde(default)]
    updates: Vec<RawSlotUpdate>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawRewriteBatchResponse {
    batch_id: String,
    #[serde(default)]
    results: Vec<RawRewriteUnitResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawSlotUpdate {
    slot_id: String,
    text: String,
}

pub fn parse_rewrite_unit_response(
    request: &RewriteUnitRequest,
    raw: &str,
) -> Result<RewriteUnitResponse, String> {
    let parsed = parse_raw_response(raw)?;
    validate_rewrite_unit_id(request, &parsed)?;
    validate_slot_updates(request, &parsed.updates)?;

    Ok(RewriteUnitResponse {
        rewrite_unit_id: parsed.rewrite_unit_id,
        updates: parsed
            .updates
            .into_iter()
            .map(|update| SlotUpdate::new(&update.slot_id, &update.text))
            .collect(),
    })
}

pub fn parse_rewrite_batch_response(
    request: &RewriteBatchRequest,
    raw: &str,
) -> Result<RewriteBatchResponse, String> {
    let parsed = parse_raw_batch_response(raw)?;
    validate_batch_id(request, &parsed)?;
    validate_batch_results(request, &parsed.results)?;

    Ok(RewriteBatchResponse {
        batch_id: parsed.batch_id,
        results: parsed
            .results
            .into_iter()
            .map(|result| RewriteUnitResponse {
                rewrite_unit_id: result.rewrite_unit_id,
                updates: result
                    .updates
                    .into_iter()
                    .map(|update| SlotUpdate::new(&update.slot_id, &update.text))
                    .collect(),
            })
            .collect(),
    })
}

fn parse_raw_response(raw: &str) -> Result<RawRewriteUnitResponse, String> {
    serde_json::from_str(raw).map_err(|error| format!("改写单元返回不是合法 JSON：{error}"))
}

fn parse_raw_batch_response(raw: &str) -> Result<RawRewriteBatchResponse, String> {
    serde_json::from_str(raw).map_err(|error| format!("改写批次返回不是合法 JSON：{error}"))
}

fn validate_rewrite_unit_id(
    request: &RewriteUnitRequest,
    response: &RawRewriteUnitResponse,
) -> Result<(), String> {
    if response.rewrite_unit_id == request.rewrite_unit_id {
        return Ok(());
    }
    Err(format!(
        "rewriteUnitId 不匹配：期望 {}，实际 {}。",
        request.rewrite_unit_id, response.rewrite_unit_id
    ))
}

fn validate_slot_updates(
    request: &RewriteUnitRequest,
    updates: &[RawSlotUpdate],
) -> Result<(), String> {
    let slot_permissions = request
        .slots
        .iter()
        .map(|slot| (slot.slot_id.as_str(), slot.editable))
        .collect::<HashMap<_, _>>();
    let mut seen = HashSet::new();

    for update in updates {
        validate_single_slot_update(&slot_permissions, &mut seen, update)?;
    }

    Ok(())
}

fn validate_batch_id(
    request: &RewriteBatchRequest,
    response: &RawRewriteBatchResponse,
) -> Result<(), String> {
    if response.batch_id == request.batch_id {
        return Ok(());
    }
    Err(format!(
        "batchId 不匹配：期望 {}，实际 {}。",
        request.batch_id, response.batch_id
    ))
}

fn validate_batch_results(
    request: &RewriteBatchRequest,
    results: &[RawRewriteUnitResponse],
) -> Result<(), String> {
    if request.units.len() != results.len() {
        return Err(format!(
            "results 数量不匹配：期望 {}，实际 {}。",
            request.units.len(),
            results.len()
        ));
    }

    for (unit, result) in request.units.iter().zip(results.iter()) {
        validate_rewrite_unit_id(unit, result)?;
        validate_slot_updates(unit, &result.updates)?;
    }

    Ok(())
}

fn validate_single_slot_update(
    slot_permissions: &HashMap<&str, bool>,
    seen: &mut HashSet<String>,
    update: &RawSlotUpdate,
) -> Result<(), String> {
    let editable = slot_permissions
        .get(update.slot_id.as_str())
        .ok_or_else(|| format!("未知 slot_id：{}。", update.slot_id))?;
    if !editable {
        return Err(format!("locked slot 不允许修改：{}。", update.slot_id));
    }
    if !seen.insert(update.slot_id.clone()) {
        return Err(format!("slot_id 重复：{}。", update.slot_id));
    }
    Ok(())
}
