use crate::models::AppSettings;

const SYSTEM_PROMPT_FALLBACK: &str = "你是一名严谨的中文文本编辑。你的任务是对给定片段进行自然化改写，让表达更像真实人工写作，但必须保持原意、事实、语气和段落层次稳定。不要扩写，不要总结，不要解释，不要输出标题，只输出改写后的正文。";
const SYSTEM_PROMPT_AIGC_V1: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../prompt/1.txt"));
const SYSTEM_PROMPT_HUMANIZER_ZH: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../prompt/2.txt"));

// 模型偶发“跑题输出”：自我介绍/免责声明/客套话等（例如 “I am Claude ...”）。
// 这类内容会直接污染正文并进入审阅记录，因此：
// - prompt 里追加硬约束（不允许自我介绍）
// - 输出阶段做一次“验收”，不合格则自动重试（温度降为 0）并加强约束
pub(super) const EXTRA_CONSTRAINT_NO_MODEL_META: &str =
    "不要自我介绍，不要描述你是 AI/模型/助手，不要输出任何免责声明或客套话；只输出与原文对应的改写正文，不要插入与原文无关的内容。";
pub(super) const EXTRA_CONSTRAINT_NO_MODEL_META_RETRY: &str =
    "再次强调：禁止输出自我介绍/免责声明/客套话（例如“I am Claude...”“As an AI language model...”）。如果不确定该写什么，就尽量贴近原文表达。";

pub(super) fn resolve_system_prompt(settings: &AppSettings) -> String {
    let preset_id = settings.prompt_preset_id.trim();

    let base = match preset_id {
        "aigc_v1" => SYSTEM_PROMPT_AIGC_V1.trim(),
        "humanizer_zh" => SYSTEM_PROMPT_HUMANIZER_ZH.trim(),
        _ => settings
            .custom_prompts
            .iter()
            .find(|item| item.id == preset_id)
            .map(|item| item.content.trim())
            .unwrap_or(""),
    };
    let base = if base.is_empty() {
        SYSTEM_PROMPT_FALLBACK
    } else {
        base
    };

    let extra = if preset_id == "aigc_v1" {
        "补充约束：最终输出不要包含“修改后/原文”等标签，只输出改写后的正文。"
    } else {
        "补充约束：最终输出只输出改写后的正文，不要输出标题、列表或解释。"
    };

    format!("{base}\n\n{extra}")
}

pub(super) fn merge_extra_constraints(primary: Option<&str>, appended: &[&str]) -> Option<String> {
    let mut pieces: Vec<String> = Vec::new();

    if let Some(primary) = primary
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        pieces.push(primary.to_string());
    }

    for item in appended.iter() {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        pieces.push(item.to_string());
    }

    if pieces.is_empty() {
        None
    } else {
        // build_*_rewrite_prompt 会以 `- {value}` 的形式嵌入，因此这里用 `\n- ` 拼接即可形成多条 bullet。
        Some(pieces.join("\n- "))
    }
}
