use super::*;

pub(super) fn merge_adjacent_writeback_regions(
    regions: Vec<WritebackRegionTemplate>,
) -> Vec<WritebackRegionTemplate> {
    let mut merged = Vec::new();
    for region in regions {
        if let Some(last) = merged.last_mut() {
            if merge_writeback_region(last, &region) {
                continue;
            }
        }
        merged.push(region);
    }
    merged
}

pub(super) fn writeback_regions_have_visible_content(regions: &[WritebackRegionTemplate]) -> bool {
    regions
        .iter()
        .any(|region| text_has_visible_content(region.text()))
}

pub(super) fn text_has_visible_content(text: &str) -> bool {
    !text.trim().is_empty()
}

pub(super) fn merge_writeback_region(
    current: &mut WritebackRegionTemplate,
    next: &WritebackRegionTemplate,
) -> bool {
    match (current, next) {
        (WritebackRegionTemplate::Editable(current), WritebackRegionTemplate::Editable(next)) => {
            if !can_merge_editable_writeback_regions(current, next) {
                return false;
            }
            current.text.push_str(&next.text);
            true
        }
        (WritebackRegionTemplate::Locked(current), WritebackRegionTemplate::Locked(next)) => {
            merge_locked_writeback_regions(current, next)
        }
        _ => false,
    }
}

pub(super) fn merge_locked_writeback_regions(
    current: &mut LockedRegionTemplate,
    next: &LockedRegionTemplate,
) -> bool {
    if current.presentation != next.presentation || current.display_mode != next.display_mode {
        return false;
    }
    current.text.push_str(&next.text);
    current.render = merge_locked_region_render(current.render.clone(), next.render.clone());
    true
}

pub(super) fn merge_locked_region_render(
    current: LockedRegionRender,
    next: LockedRegionRender,
) -> LockedRegionRender {
    let mut items = Vec::new();
    extend_locked_region_render(&mut items, current);
    extend_locked_region_render(&mut items, next);
    LockedRegionRender::Sequence(items)
}

pub(super) fn extend_locked_region_render(items: &mut Vec<LockedRegionRender>, render: LockedRegionRender) {
    match render {
        LockedRegionRender::Sequence(nested) => items.extend(nested),
        other => items.push(other),
    }
}

pub(super) fn can_merge_editable_writeback_regions(
    current: &EditableRegionTemplate,
    next: &EditableRegionTemplate,
) -> bool {
    if current.allow_rewrite != next.allow_rewrite || current.presentation != next.presentation {
        return false;
    }
    match (&current.render, &next.render) {
        (
            EditableRegionRender::Run {
                run_property_events: current_props,
            },
            EditableRegionRender::Run {
                run_property_events: next_props,
            },
        ) => run_property_signature(current_props) == run_property_signature(next_props),
        (
            EditableRegionRender::Hyperlink {
                hyperlink_start: current_start,
                run_property_events: current_props,
            },
            EditableRegionRender::Hyperlink {
                hyperlink_start: next_start,
                run_property_events: next_props,
            },
        ) => {
            current_start == next_start
                && run_property_signature(current_props) == run_property_signature(next_props)
        }
        _ => false,
    }
}

