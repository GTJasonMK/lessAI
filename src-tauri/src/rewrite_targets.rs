use std::collections::{HashSet, VecDeque};

use crate::models::RewriteUnitStatus;
use crate::rewrite_unit::RewriteUnit;

pub fn resolve_target_rewrite_unit_ids(
    units: &[RewriteUnit],
    target_rewrite_unit_ids: Option<Vec<String>>,
) -> Result<Option<HashSet<String>>, String> {
    let Some(unit_ids) = target_rewrite_unit_ids else {
        return Ok(None);
    };

    let mut selected = HashSet::new();
    for unit_id in unit_ids {
        let Some(unit) = units.iter().find(|unit| unit.id == unit_id) else {
            return Err(format!("所选改写单元不存在：{unit_id}"));
        };
        if unit.status == RewriteUnitStatus::Done {
            continue;
        }
        selected.insert(unit.id.clone());
    }

    if selected.is_empty() {
        return Err("所选改写单元均不可改写。".to_string());
    }

    Ok(Some(selected))
}

pub fn find_next_manual_batch(
    units: &[RewriteUnit],
    target_unit_ids: Option<&HashSet<String>>,
    batch_size: usize,
) -> Vec<String> {
    units
        .iter()
        .filter(|unit| {
            is_target_unit(target_unit_ids, &unit.id)
                && matches!(unit.status, RewriteUnitStatus::Idle | RewriteUnitStatus::Failed)
        })
        .take(batch_size.max(1))
        .map(|unit| unit.id.clone())
        .collect()
}

pub fn build_auto_pending_queue(
    units: &[RewriteUnit],
    target_unit_ids: Option<&HashSet<String>>,
) -> VecDeque<String> {
    units
        .iter()
        .filter(|unit| is_target_unit(target_unit_ids, &unit.id) && unit.status != RewriteUnitStatus::Done)
        .map(|unit| unit.id.clone())
        .collect()
}

pub fn take_next_auto_batch(pending: &mut VecDeque<String>, batch_size: usize) -> Vec<String> {
    let mut batch = Vec::new();
    while batch.len() < batch_size.max(1) {
        let Some(unit_id) = pending.pop_front() else {
            break;
        };
        batch.push(unit_id);
    }
    batch
}

pub fn count_target_total_units(
    units: &[RewriteUnit],
    target_unit_ids: Option<&HashSet<String>>,
) -> usize {
    units
        .iter()
        .filter(|unit| is_target_unit(target_unit_ids, &unit.id))
        .count()
}

pub fn count_target_completed_units(
    units: &[RewriteUnit],
    target_unit_ids: Option<&HashSet<String>>,
) -> usize {
    units
        .iter()
        .filter(|unit| is_target_unit(target_unit_ids, &unit.id) && unit.status == RewriteUnitStatus::Done)
        .count()
}

fn is_target_unit(target_unit_ids: Option<&HashSet<String>>, unit_id: &str) -> bool {
    match target_unit_ids {
        Some(unit_ids) => unit_ids.contains(unit_id),
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::{
        build_auto_pending_queue, count_target_completed_units, count_target_total_units,
        find_next_manual_batch, resolve_target_rewrite_unit_ids, take_next_auto_batch,
    };
    use crate::{
        models::{SegmentationPreset, RewriteUnitStatus},
        rewrite_unit::RewriteUnit,
    };

    fn unit(id: &str, status: RewriteUnitStatus) -> RewriteUnit {
        RewriteUnit {
            id: id.to_string(),
            order: 0,
            slot_ids: vec![format!("slot-{id}")],
            display_text: id.to_string(),
            segmentation_preset: SegmentationPreset::Paragraph,
            status,
            error_message: None,
        }
    }

    #[test]
    fn resolve_target_rewrite_unit_ids_keeps_only_selected_rewriteable_units() {
        let units = vec![
            unit("unit-0", RewriteUnitStatus::Done),
            unit("unit-1", RewriteUnitStatus::Idle),
            unit("unit-2", RewriteUnitStatus::Failed),
        ];

        let selected = resolve_target_rewrite_unit_ids(
            &units,
            Some(vec!["unit-0".to_string(), "unit-2".to_string()]),
        )
        .unwrap();

        let mut actual = selected.unwrap().into_iter().collect::<Vec<_>>();
        actual.sort();
        assert_eq!(actual, vec!["unit-2".to_string()]);
    }

    #[test]
    fn resolve_target_rewrite_unit_ids_rejects_missing_unit() {
        let error = resolve_target_rewrite_unit_ids(&[unit("unit-0", RewriteUnitStatus::Idle)], Some(vec![
            "unit-x".to_string(),
        ]))
        .expect_err("missing unit should fail");

        assert!(error.contains("不存在"));
    }

    #[test]
    fn find_next_manual_batch_returns_target_units_in_order() {
        let units = vec![
            unit("unit-0", RewriteUnitStatus::Idle),
            unit("unit-1", RewriteUnitStatus::Failed),
            unit("unit-2", RewriteUnitStatus::Done),
        ];
        let selected =
            resolve_target_rewrite_unit_ids(&units, Some(vec!["unit-1".to_string()])).unwrap();

        assert_eq!(find_next_manual_batch(&units, selected.as_ref(), 2), vec!["unit-1"]);
    }

    #[test]
    fn take_next_auto_batch_pops_pending_unit_ids_in_order() {
        let mut pending = VecDeque::from(vec!["unit-2".to_string(), "unit-4".to_string()]);

        assert_eq!(
            take_next_auto_batch(&mut pending, 1),
            vec!["unit-2".to_string()]
        );
        assert_eq!(pending.into_iter().collect::<Vec<_>>(), vec!["unit-4"]);
    }

    #[test]
    fn target_counts_are_scoped_to_selected_units() {
        let units = vec![
            unit("unit-0", RewriteUnitStatus::Done),
            unit("unit-1", RewriteUnitStatus::Idle),
            unit("unit-2", RewriteUnitStatus::Done),
        ];
        let selected = resolve_target_rewrite_unit_ids(
            &units,
            Some(vec!["unit-1".to_string(), "unit-2".to_string()]),
        )
        .unwrap();

        assert_eq!(count_target_total_units(&units, selected.as_ref()), 1);
        assert_eq!(count_target_completed_units(&units, selected.as_ref()), 0);
    }

    #[test]
    fn build_auto_pending_queue_uses_only_selected_units() {
        let units = vec![
            unit("unit-0", RewriteUnitStatus::Idle),
            unit("unit-1", RewriteUnitStatus::Done),
            unit("unit-2", RewriteUnitStatus::Failed),
        ];
        let selected = resolve_target_rewrite_unit_ids(
            &units,
            Some(vec!["unit-1".to_string(), "unit-2".to_string()]),
        )
        .unwrap();

        assert_eq!(
            build_auto_pending_queue(&units, selected.as_ref())
                .into_iter()
                .collect::<Vec<_>>(),
            vec!["unit-2".to_string()]
        );
    }
}
