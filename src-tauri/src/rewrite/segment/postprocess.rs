use crate::rewrite::SegmentedChunk;

const LEFT_BINDING_PUNCTUATION: &[char] = &[
    '，', ',', '、', '：', ':', '；', ';', '。', '.', '！', '!', '？', '?', '）', ')', '】', ']',
    '}', '」', '』', '》', '〉', '”', '’', '"', '\'',
];

pub(super) fn merge_left_binding_punctuation_chunks(
    chunks: Vec<SegmentedChunk>,
) -> Vec<SegmentedChunk> {
    let mut merged: Vec<SegmentedChunk> = Vec::with_capacity(chunks.len());

    for chunk in chunks {
        if try_merge_into_previous(&mut merged, &chunk) {
            continue;
        }
        merged.push(chunk);
    }

    merged
}

fn try_merge_into_previous(merged: &mut [SegmentedChunk], chunk: &SegmentedChunk) -> bool {
    let Some(previous) = merged.last_mut() else {
        return false;
    };
    if !can_merge_left_binding_punctuation(previous, chunk) {
        return false;
    }

    previous.text.push_str(&chunk.text);
    previous.separator_after = chunk.separator_after.clone();
    true
}

fn can_merge_left_binding_punctuation(previous: &SegmentedChunk, chunk: &SegmentedChunk) -> bool {
    previous.skip_rewrite == chunk.skip_rewrite
        && previous.presentation == chunk.presentation
        && previous.separator_after.is_empty()
        && is_left_binding_punctuation_chunk(&chunk.text)
}

fn is_left_binding_punctuation_chunk(text: &str) -> bool {
    !text.is_empty() && text.chars().all(is_left_binding_punctuation)
}

fn is_left_binding_punctuation(ch: char) -> bool {
    LEFT_BINDING_PUNCTUATION.contains(&ch)
}
