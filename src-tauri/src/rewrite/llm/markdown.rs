use crate::adapters::markdown::MarkdownAdapter;
use crate::adapters::TextRegion;
use crate::models::AppSettings;

pub(super) async fn rewrite_markdown_chunk_with_client(
    client: &reqwest::Client,
    settings: &AppSettings,
    source_text: &str,
) -> Result<String, String> {
    // Markdown 片段常包含“语法强约束片段”（链接/内联代码/强调标记/公式/HTML/引用定义等）。
    // 如果把这些内容直接交给模型改写，极易改坏语法，导致渲染/导出漂移。
    //
    // 目标：
    // - chunk 作为审阅单元应保持自然（句/段），不被保护区切碎；
    // - 但改写时必须锁定保护区，保证拼回去后 Markdown 结构严格不变。
    //
    // 策略（与 TeX 类似）：
    // - 先用 MarkdownAdapter 标记 skip_rewrite 片段；
    // - 若 skip 片段跨行（包含换行），退回到“按 region 改写可改写区域”的保守模式；
    // - 否则用占位符替换 skip 片段，让模型只改写正文，再把占位符替换回原始片段。
    let regions = MarkdownAdapter::split_regions(source_text, settings.rewrite_headings);
    if regions.iter().all(|region| !region.skip_rewrite) {
        return super::plain::rewrite_plain_chunk_with_client(client, settings, source_text, None)
            .await;
    }

    // 多行结构（例如 fenced code block / table / front matter / 多行 HTML 注释）：
    // 用“按 region 改写可改写区域”的保守模式，避免占位符被模型搬家导致结构块漂移。
    let has_multiline_skip = regions.iter().any(|region| {
        if !region.skip_rewrite {
            return false;
        }
        let trimmed = region.body.trim_end_matches(|ch: char| ch.is_whitespace());
        trimmed.contains('\n') || trimmed.contains('\r')
    });
    if has_multiline_skip {
        return rewrite_markdown_chunk_by_regions(client, settings, regions).await;
    }

    let (masked, placeholders) = mask_markdown_regions_with_placeholders(&regions);
    if placeholders.is_empty() {
        return super::plain::rewrite_plain_chunk_with_client(client, settings, source_text, None)
            .await;
    }

    let placeholder_rule = "文本中可能包含形如 ⟦LESSAI_LOCK_1⟧ 的占位符。必须逐字原样保留它们（不得改动/不得删除/不得复制到别处/不得移动顺序）。";
    let candidate_masked = super::plain::rewrite_plain_chunk_with_client(
        client,
        settings,
        &masked,
        Some(placeholder_rule),
    )
    .await?;

    // 占位符验收：
    // - 必须每个占位符都出现且仅出现一次（避免模型复制/丢失导致锁定片段重复或缺失）。
    let mut placeholders_ok = true;
    let mut search_from = 0usize;
    for (placeholder, _) in placeholders.iter() {
        let count = candidate_masked.matches(placeholder).count();
        if count != 1 {
            placeholders_ok = false;
            break;
        }

        let Some(pos) = candidate_masked[search_from..].find(placeholder) else {
            placeholders_ok = false;
            break;
        };
        search_from = search_from
            .saturating_add(pos)
            .saturating_add(placeholder.len());
    }
    if !placeholders_ok {
        return rewrite_markdown_chunk_by_regions(client, settings, regions).await;
    }

    let mut rebuilt = candidate_masked;
    for (placeholder, original) in placeholders.into_iter() {
        rebuilt = rebuilt.replace(&placeholder, &original);
    }

    Ok(rebuilt)
}

async fn rewrite_markdown_chunk_by_regions(
    client: &reqwest::Client,
    settings: &AppSettings,
    regions: Vec<TextRegion>,
) -> Result<String, String> {
    let mut out = String::new();
    for region in regions.into_iter() {
        if region.skip_rewrite {
            out.push_str(&region.body);
            continue;
        }

        let rewritten =
            super::plain::rewrite_plain_chunk_with_client(client, settings, &region.body, None)
                .await?;
        out.push_str(&rewritten);
    }
    Ok(out)
}

fn mask_markdown_regions_with_placeholders(
    regions: &[TextRegion],
) -> (String, Vec<(String, String)>) {
    let mut masked = String::new();
    let mut placeholders: Vec<(String, String)> = Vec::new();
    let mut seq = 1usize;

    for region in regions.iter() {
        if !region.skip_rewrite {
            masked.push_str(&region.body);
            continue;
        }

        let placeholder = format!("⟦LESSAI_LOCK_{seq}⟧");
        seq = seq.saturating_add(1);
        placeholders.push((placeholder.clone(), region.body.clone()));
        masked.push_str(&placeholder);
    }

    (masked, placeholders)
}
