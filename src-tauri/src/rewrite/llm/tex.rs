use crate::adapters::tex::TexAdapter;
use crate::adapters::TextRegion;
use crate::models::AppSettings;

pub(super) async fn rewrite_tex_chunk_with_client(
    client: &reqwest::Client,
    settings: &AppSettings,
    source_text: &str,
) -> Result<String, String> {
    // TeX 片段中会夹杂大量语法结构（命令/数学/注释/环境等），直接让 LLM 改写非常容易破坏可编译性。
    //
    // 这里采用“可改写正文 + 锁定占位符”的策略：
    // - 先用 TexAdapter 识别不可改写片段（skip_rewrite=true）；
    // - 若这些片段都在单行内（不包含换行），则用占位符替换后交给模型改写；
    // - 模型输出后再把占位符替换回原始片段，保证语法严格保真；
    // - 若存在跨行的 skip 片段（环境/块数学/注释等），退回到“分段改写可改写区域”的保守模式。
    let regions = TexAdapter::split_regions(source_text, settings.rewrite_headings);
    if regions.iter().all(|region| !region.skip_rewrite) {
        return super::plain::rewrite_plain_chunk_with_client(client, settings, source_text, None)
            .await;
    }

    // 多行强约束片段（环境/块数学/多行注释等）：
    // 用保守模式避免占位符被模型搬家导致结构漂移。
    let has_multiline_skip = regions.iter().any(|region| {
        if !region.skip_rewrite {
            return false;
        }
        let trimmed = region.body.trim_end_matches(|ch: char| ch.is_whitespace());
        trimmed.contains('\n') || trimmed.contains('\r')
    });
    if has_multiline_skip {
        return rewrite_tex_chunk_by_regions(client, settings, regions).await;
    }

    let (masked, placeholders) = mask_tex_regions_with_placeholders(&regions);
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
    // - 必须每个占位符都出现且仅出现一次（避免复制/丢失导致锁定片段重复或缺失）
    // - 必须按输入顺序出现（避免锁定片段被搬家）
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
        // 占位符被模型改坏：退回到保守模式，不信任该输出。
        return rewrite_tex_chunk_by_regions(client, settings, regions).await;
    }

    let mut rebuilt = candidate_masked;
    for (placeholder, original) in placeholders.into_iter() {
        rebuilt = rebuilt.replace(&placeholder, &original);
    }

    Ok(rebuilt)
}

async fn rewrite_tex_chunk_by_regions(
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

fn mask_tex_regions_with_placeholders(regions: &[TextRegion]) -> (String, Vec<(String, String)>) {
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
