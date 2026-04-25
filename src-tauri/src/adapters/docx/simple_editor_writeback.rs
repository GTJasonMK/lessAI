use super::*;

pub(super) fn split_updated_paragraphs(
    updated_text: &str,
    expected_count: usize,
) -> Result<Vec<String>, String> {
    let normalized = updated_text.replace("\r\n", "\n").replace('\r', "\n");

    let paragraphs = if expected_count == 0 && normalized.is_empty() {
        Vec::new()
    } else {
        normalized
            .split("\n\n")
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
    };

    if paragraphs.len() != expected_count {
        return Err(
            "当前简单 docx 写回要求段落数量保持不变，请不要新增、删除或合并段落。".to_string(),
        );
    }
    Ok(paragraphs)
}

pub(super) fn build_editor_writeback_updated_regions(
    blocks: &[WritebackBlockTemplate],
    updated_text: &str,
) -> Result<Vec<TextRegion>, String> {
    validate_editor_writeback_blocks(blocks)?;
    let display_blocks = build_display_blocks(blocks);
    let updated_paragraphs = split_updated_paragraphs(updated_text, display_blocks.len())?;
    let mut regions = Vec::new();

    for (index, display_block) in display_blocks.iter().enumerate() {
        let updated_paragraph = updated_paragraphs
            .get(index)
            .ok_or_else(|| "docx 段落数量与写回内容不一致，无法生成 document.xml。".to_string())?
            .clone();
        let append_block_separator = index + 1 < display_blocks.len();
        let mut block_regions = match display_block.kind {
            DisplayBlockKind::Paragraph { block_index } => {
                let WritebackBlockTemplate::Paragraph(paragraph) = &blocks[block_index] else {
                    continue;
                };
                build_editor_writeback_paragraph_display_regions(
                    paragraph,
                    &display_block.region_indices,
                    &updated_paragraph,
                )?
            }
            DisplayBlockKind::LockedBlock { block_index } => {
                let WritebackBlockTemplate::Locked(region) = &blocks[block_index] else {
                    continue;
                };
                vec![locked_region_from_presentation(
                    updated_paragraph,
                    region.presentation.clone(),
                )]
            }
        };
        if let Some(last) = block_regions.last_mut() {
            if append_block_separator {
                last.body.push_str(DOCX_BLOCK_SEPARATOR);
            }
        }
        regions.extend(block_regions);
    }

    Ok(regions)
}

pub(super) fn validate_editor_writeback_blocks(blocks: &[WritebackBlockTemplate]) -> Result<(), String> {
    for block in blocks {
        if let WritebackBlockTemplate::Paragraph(paragraph) = block {
            validate_editor_writeback_paragraph(paragraph)?;
        }
    }
    Ok(())
}

pub(super) fn build_editor_writeback_paragraph_display_regions(
    paragraph: &WritebackParagraphTemplate,
    region_indices: &[usize],
    updated_text: &str,
) -> Result<Vec<TextRegion>, String> {
    if region_indices.is_empty() {
        return Ok(vec![TextRegion::editable(updated_text.to_string())]);
    }

    let template = region_indices
        .iter()
        .filter_map(|region_index| paragraph.regions.get(*region_index).cloned())
        .collect::<Vec<_>>();
    build_editor_writeback_paragraph_regions_from_template(&template, updated_text)
}

pub(super) fn build_editor_writeback_paragraph_regions_from_template(
    template: &[WritebackRegionTemplate],
    updated_text: &str,
) -> Result<Vec<TextRegion>, String> {
    if template.is_empty() {
        return Ok(vec![TextRegion::editable(updated_text.to_string())]);
    }

    let mut matches = Vec::new();
    collect_editor_writeback_paragraph_matches(
        template,
        updated_text,
        0,
        0,
        Vec::new(),
        &mut matches,
        2,
    );

    match matches.len() {
        1 => Ok(matches.remove(0)),
        0 => Err("写回内容越过原有样式或锁定边界，无法安全写回。".to_string()),
        _ => Err("写回内容在原有样式或锁定边界上的定位存在歧义，无法安全写回。".to_string()),
    }
}

pub(super) fn validate_editor_writeback_paragraph(
    _paragraph: &WritebackParagraphTemplate,
) -> Result<(), String> {
    Ok(())
}

pub(super) fn collect_editor_writeback_paragraph_matches(
    template: &[WritebackRegionTemplate],
    updated_text: &str,
    region_index: usize,
    cursor: usize,
    current: Vec<TextRegion>,
    matches: &mut Vec<Vec<TextRegion>>,
    limit: usize,
) {
    if matches.len() >= limit {
        return;
    }
    if region_index >= template.len() {
        if cursor == updated_text.len() {
            matches.push(current);
        }
        return;
    }

    if template[region_index].skip_rewrite() {
        let locked_text = template[region_index].text().to_string();
        let Some(rest) = updated_text.get(cursor..) else {
            return;
        };
        if !rest.starts_with(&locked_text) {
            return;
        }
        let mut next = current;
        next.push(logical_locked_text_region(&template[region_index]));
        collect_editor_writeback_paragraph_matches(
            template,
            updated_text,
            region_index + 1,
            cursor + locked_text.len(),
            next,
            matches,
            limit,
        );
        return;
    }

    collect_editor_writeback_editable_group_matches(
        template,
        updated_text,
        region_index,
        cursor,
        current,
        matches,
        limit,
    );
}

#[derive(Default)]
pub(super) struct PlainTextEditorEditBlock {
    owner: Option<usize>,
    inserted_text: String,
}

pub(super) fn collect_editor_writeback_editable_group_matches(
    template: &[WritebackRegionTemplate],
    updated_text: &str,
    region_index: usize,
    cursor: usize,
    current: Vec<TextRegion>,
    matches: &mut Vec<Vec<TextRegion>>,
    limit: usize,
) {
    let group_end = next_locked_region_index(template, region_index);
    let group = &template[region_index..group_end];

    if let Some(next_locked) = template.get(group_end) {
        let locked_text = next_locked.text();
        if locked_text.is_empty() {
            return;
        }
        let Some(rest) = updated_text.get(cursor..) else {
            return;
        };
        for (relative_index, _) in rest.match_indices(locked_text) {
            let next_cursor = cursor + relative_index;
            let Some(mapped) =
                map_editable_group_regions(group, &updated_text[cursor..next_cursor])
            else {
                continue;
            };
            let mut next = current.clone();
            next.extend(mapped);
            collect_editor_writeback_paragraph_matches(
                template,
                updated_text,
                group_end,
                next_cursor,
                next,
                matches,
                limit,
            );
            if matches.len() >= limit {
                return;
            }
        }
        return;
    }

    let Some(mapped) = map_editable_group_regions(group, &updated_text[cursor..]) else {
        return;
    };
    let mut completed = current;
    completed.extend(mapped);
    matches.push(completed);
}

pub(super) fn next_locked_region_index(template: &[WritebackRegionTemplate], start: usize) -> usize {
    let mut index = start;
    while index < template.len() {
        if template[index].skip_rewrite() {
            return index;
        }
        index += 1;
    }
    template.len()
}

pub(super) fn logical_locked_text_region(region: &WritebackRegionTemplate) -> TextRegion {
    match region {
        WritebackRegionTemplate::Editable(editable) => {
            locked_region_from_presentation(editable.text.clone(), editable.presentation.clone())
        }
        WritebackRegionTemplate::Locked(locked) => {
            locked_region_from_presentation(locked.text.clone(), locked.presentation.clone())
        }
    }
}

pub(super) fn map_editable_group_regions(
    group: &[WritebackRegionTemplate],
    updated_text: &str,
) -> Option<Vec<TextRegion>> {
    let editable_regions = editable_group_templates(group)?;
    if editable_regions.len() == 1 {
        return Some(vec![editable_region_text(
            editable_regions[0],
            updated_text.to_string(),
        )]);
    }

    let original_text = editable_regions
        .iter()
        .map(|region| region.text.as_str())
        .collect::<String>();
    if original_text.is_empty() {
        return map_empty_editable_group_regions(&editable_regions, updated_text);
    }

    let boundaries = editable_group_char_boundaries(&editable_regions);
    let original_len = original_text.chars().count();
    let mut outputs = vec![String::new(); editable_regions.len()];
    let mut original_index = 0usize;
    let mut block = PlainTextEditorEditBlock::default();

    for span in crate::rewrite::build_diff(&original_text, updated_text) {
        if !apply_diff_span_to_editable_group(
            &mut outputs,
            &mut block,
            &boundaries,
            original_len,
            &mut original_index,
            span.r#type,
            &span.text,
        ) {
            return None;
        }
    }

    if !flush_editor_writeback_edit_block(&mut outputs, &mut block) {
        return None;
    }
    if original_index != original_len {
        return None;
    }

    Some(
        outputs
            .into_iter()
            .zip(editable_regions)
            .map(|(body, region)| editable_region_text(region, body))
            .collect(),
    )
}

pub(super) fn editable_group_templates(
    group: &[WritebackRegionTemplate],
) -> Option<Vec<&EditableRegionTemplate>> {
    group
        .iter()
        .map(|region| match region {
            WritebackRegionTemplate::Editable(region) if region.allow_rewrite => Some(region),
            _ => None,
        })
        .collect()
}

pub(super) fn map_empty_editable_group_regions(
    editable_regions: &[&EditableRegionTemplate],
    updated_text: &str,
) -> Option<Vec<TextRegion>> {
    if updated_text.is_empty() {
        return Some(
            editable_regions
                .iter()
                .map(|region| editable_region_text(region, String::new()))
                .collect(),
        );
    }
    if editable_regions.len() == 1 {
        return Some(vec![editable_region_text(
            editable_regions[0],
            updated_text.to_string(),
        )]);
    }
    None
}

pub(super) fn editable_group_char_boundaries(editable_regions: &[&EditableRegionTemplate]) -> Vec<usize> {
    let mut boundaries = Vec::with_capacity(editable_regions.len());
    let mut total = 0usize;
    for region in editable_regions {
        total += region.text.chars().count();
        boundaries.push(total);
    }
    boundaries
}

pub(super) fn apply_diff_span_to_editable_group(
    outputs: &mut [String],
    block: &mut PlainTextEditorEditBlock,
    boundaries: &[usize],
    original_len: usize,
    original_index: &mut usize,
    diff_type: DiffType,
    text: &str,
) -> bool {
    match diff_type {
        DiffType::Unchanged => {
            apply_unchanged_editable_text(outputs, block, boundaries, original_index, text)
        }
        DiffType::Delete => apply_deleted_editable_text(block, boundaries, original_index, text),
        DiffType::Insert => {
            apply_inserted_editable_text(block, boundaries, original_len, *original_index, text)
        }
    }
}

pub(super) fn apply_unchanged_editable_text(
    outputs: &mut [String],
    block: &mut PlainTextEditorEditBlock,
    boundaries: &[usize],
    original_index: &mut usize,
    text: &str,
) -> bool {
    if !flush_editor_writeback_edit_block(outputs, block) {
        return false;
    }
    for ch in text.chars() {
        let Some(region_index) = region_index_for_original_char(boundaries, *original_index) else {
            return false;
        };
        outputs[region_index].push(ch);
        *original_index += 1;
    }
    true
}

pub(super) fn apply_deleted_editable_text(
    block: &mut PlainTextEditorEditBlock,
    boundaries: &[usize],
    original_index: &mut usize,
    text: &str,
) -> bool {
    for _ in text.chars() {
        let Some(region_index) = region_index_for_original_char(boundaries, *original_index) else {
            return false;
        };
        add_editor_writeback_block_owner(block, region_index);
        *original_index += 1;
    }
    true
}

pub(super) fn apply_inserted_editable_text(
    block: &mut PlainTextEditorEditBlock,
    boundaries: &[usize],
    original_len: usize,
    original_index: usize,
    text: &str,
) -> bool {
    let Some(region_index) = insertion_region_index(boundaries, original_len, original_index)
    else {
        return false;
    };
    add_editor_writeback_block_owner(block, region_index);
    block.inserted_text.push_str(text);
    true
}

pub(super) fn flush_editor_writeback_edit_block(
    outputs: &mut [String],
    block: &mut PlainTextEditorEditBlock,
) -> bool {
    if let Some(owner) = block.owner {
        outputs[owner].push_str(&block.inserted_text);
    } else if !block.inserted_text.is_empty() {
        return false;
    }
    *block = PlainTextEditorEditBlock::default();
    true
}

pub(super) fn add_editor_writeback_block_owner(block: &mut PlainTextEditorEditBlock, owner: usize) {
    match block.owner {
        Some(_) => {}
        None => block.owner = Some(owner),
    }
}

pub(super) fn region_index_for_original_char(boundaries: &[usize], original_index: usize) -> Option<usize> {
    boundaries.iter().position(|end| original_index < *end)
}

pub(super) fn insertion_region_index(
    boundaries: &[usize],
    original_len: usize,
    original_index: usize,
) -> Option<usize> {
    if boundaries.is_empty() {
        return None;
    }
    if original_index == 0 {
        return Some(0);
    }
    if original_index == original_len {
        return Some(boundaries.len() - 1);
    }

    let left = region_index_for_original_char(boundaries, original_index - 1)?;
    let right = region_index_for_original_char(boundaries, original_index)?;
    if left == right {
        return Some(left);
    }
    Some(left)
}

pub(super) fn editable_region_text(region: &EditableRegionTemplate, body: String) -> TextRegion {
    if region.allow_rewrite {
        return TextRegion::editable(body).with_presentation(region.presentation.clone());
    }
    locked_region_from_presentation(body, region.presentation.clone())
}

