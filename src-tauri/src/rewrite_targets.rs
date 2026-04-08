use std::collections::{HashSet, VecDeque};

use crate::models::{ChunkStatus, ChunkTask};

pub fn resolve_target_indices(
    chunks: &[ChunkTask],
    target_chunk_indices: Option<Vec<usize>>,
) -> Result<Option<HashSet<usize>>, String> {
    let Some(indices) = target_chunk_indices else {
        return Ok(None);
    };

    let mut selected = HashSet::new();
    for index in indices {
        let Some(chunk) = chunks.get(index) else {
            return Err(format!("所选片段索引越界：{index}"));
        };
        if chunk.skip_rewrite {
            continue;
        }
        selected.insert(index);
    }

    if selected.is_empty() {
        return Err("所选片段均不可改写。".to_string());
    }

    Ok(Some(selected))
}

pub fn find_next_manual_chunk(
    chunks: &[ChunkTask],
    target_indices: Option<&HashSet<usize>>,
) -> Option<usize> {
    chunks
        .iter()
        .find(|chunk| {
            !chunk.skip_rewrite
                && is_target_chunk(target_indices, chunk.index)
                && matches!(chunk.status, ChunkStatus::Idle | ChunkStatus::Failed)
        })
        .map(|chunk| chunk.index)
}

pub fn build_auto_pending_queue(
    chunks: &[ChunkTask],
    target_indices: Option<&HashSet<usize>>,
) -> VecDeque<usize> {
    chunks
        .iter()
        .filter(|chunk| {
            !chunk.skip_rewrite
                && is_target_chunk(target_indices, chunk.index)
                && chunk.status != ChunkStatus::Done
        })
        .map(|chunk| chunk.index)
        .collect()
}

pub fn count_target_total_chunks(
    chunks: &[ChunkTask],
    target_indices: Option<&HashSet<usize>>,
) -> usize {
    chunks
        .iter()
        .filter(|chunk| !chunk.skip_rewrite && is_target_chunk(target_indices, chunk.index))
        .count()
}

pub fn count_target_completed_chunks(
    chunks: &[ChunkTask],
    target_indices: Option<&HashSet<usize>>,
) -> usize {
    chunks
        .iter()
        .filter(|chunk| {
            !chunk.skip_rewrite
                && is_target_chunk(target_indices, chunk.index)
                && chunk.status == ChunkStatus::Done
        })
        .count()
}

fn is_target_chunk(target_indices: Option<&HashSet<usize>>, index: usize) -> bool {
    match target_indices {
        Some(indices) => indices.contains(&index),
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_auto_pending_queue, count_target_completed_chunks, count_target_total_chunks,
        find_next_manual_chunk, resolve_target_indices,
    };
    use crate::models::{ChunkStatus, ChunkTask};

    fn chunk(index: usize, status: ChunkStatus, skip_rewrite: bool) -> ChunkTask {
        ChunkTask {
            index,
            source_text: format!("chunk-{index}"),
            separator_after: "\n".to_string(),
            skip_rewrite,
            status,
            error_message: None,
        }
    }

    #[test]
    fn resolve_target_indices_keeps_only_selected_rewriteable_chunks() {
        let chunks = vec![
            chunk(0, ChunkStatus::Done, false),
            chunk(1, ChunkStatus::Idle, true),
            chunk(2, ChunkStatus::Failed, false),
            chunk(3, ChunkStatus::Idle, false),
        ];

        let selected = resolve_target_indices(&chunks, Some(vec![1, 2, 3])).unwrap();

        let mut actual = selected.unwrap().into_iter().collect::<Vec<_>>();
        actual.sort_unstable();
        assert_eq!(actual, vec![2, 3]);
    }

    #[test]
    fn resolve_target_indices_rejects_out_of_range_indices() {
        let chunks = vec![chunk(0, ChunkStatus::Idle, false)];

        let error = resolve_target_indices(&chunks, Some(vec![2])).unwrap_err();

        assert!(error.contains("越界"));
    }

    #[test]
    fn find_next_manual_chunk_respects_target_subset() {
        let chunks = vec![
            chunk(0, ChunkStatus::Idle, false),
            chunk(1, ChunkStatus::Failed, false),
            chunk(2, ChunkStatus::Idle, false),
        ];
        let selected = resolve_target_indices(&chunks, Some(vec![2])).unwrap();

        let next = find_next_manual_chunk(&chunks, selected.as_ref());

        assert_eq!(next, Some(2));
    }

    #[test]
    fn build_auto_pending_queue_uses_only_selected_chunks() {
        let chunks = vec![
            chunk(0, ChunkStatus::Idle, false),
            chunk(1, ChunkStatus::Done, false),
            chunk(2, ChunkStatus::Failed, false),
            chunk(3, ChunkStatus::Idle, false),
        ];
        let selected = resolve_target_indices(&chunks, Some(vec![1, 2])).unwrap();

        let pending = build_auto_pending_queue(&chunks, selected.as_ref());

        assert_eq!(pending.into_iter().collect::<Vec<_>>(), vec![2]);
    }

    #[test]
    fn target_counts_are_scoped_to_selected_chunks() {
        let chunks = vec![
            chunk(0, ChunkStatus::Done, false),
            chunk(1, ChunkStatus::Idle, false),
            chunk(2, ChunkStatus::Done, false),
            chunk(3, ChunkStatus::Idle, false),
        ];
        let selected = resolve_target_indices(&chunks, Some(vec![1, 2])).unwrap();

        assert_eq!(count_target_total_chunks(&chunks, selected.as_ref()), 2);
        assert_eq!(count_target_completed_chunks(&chunks, selected.as_ref()), 1);
    }
}
