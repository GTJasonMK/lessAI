use crate::models::{DiffSpan, DiffType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiffOpKind {
    Equal,
    Insert,
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DiffOp<T: Copy> {
    kind: DiffOpKind,
    value: T,
}

fn push_span_text(spans: &mut Vec<DiffSpan>, kind: DiffType, text: &str) {
    if text.is_empty() {
        return;
    }
    if let Some(last) = spans.last_mut() {
        if last.r#type == kind {
            last.text.push_str(text);
            return;
        }
    }
    spans.push(DiffSpan {
        r#type: kind,
        text: text.to_string(),
    });
}

fn myers_diff<T: Eq + Copy>(before: &[T], after: &[T], max_trace_bytes: usize) -> Vec<DiffOp<T>> {
    let n = before.len();
    let m = after.len();
    let max = n.saturating_add(m);

    if max == 0 {
        return Vec::new();
    }

    // trace 是 diff 算法的主要内存来源：每一轮 d 都会保存一份 v 的快照用于回溯。
    // 对“超长文本但改动很小”的场景，实际 d 很小，内存也很小；因此不使用最坏情况估算直接拒绝，
    // 而是按实际 trace 增长动态止损，超过阈值时回退到前后缀 diff（保命优先，避免 OOM）。
    let v_len = 2usize.saturating_mul(max).saturating_add(1);
    let bytes_per_trace = v_len.saturating_mul(std::mem::size_of::<i32>());
    if bytes_per_trace > max_trace_bytes {
        return myers_prefix_suffix_diff(before, after);
    }
    let mut trace_bytes_used = 0usize;

    let offset = max as i32;
    let mut v = vec![-1i32; 2 * max + 1];
    v[(offset + 1) as usize] = 0;

    let mut trace: Vec<Vec<i32>> = Vec::new();

    for d in 0..=max {
        let d_i32 = d as i32;
        let mut k = -d_i32;
        while k <= d_i32 {
            let k_index = (offset + k) as usize;

            let mut x: i32;
            if k == -d_i32
                || (k != d_i32 && v[(offset + k - 1) as usize] < v[(offset + k + 1) as usize])
            {
                // 向下走：插入 after[y]
                x = v[(offset + k + 1) as usize];
            } else {
                // 向右走：删除 before[x]
                x = v[(offset + k - 1) as usize].saturating_add(1);
            }

            let mut y: i32 = x - k;
            while (x as usize) < n && (y as usize) < m && before[x as usize] == after[y as usize] {
                x += 1;
                y += 1;
            }

            v[k_index] = x;

            if (x as usize) >= n && (y as usize) >= m {
                break;
            }

            k += 2;
        }

        if trace_bytes_used.saturating_add(bytes_per_trace) > max_trace_bytes {
            return myers_prefix_suffix_diff(before, after);
        }
        trace.push(v.clone());
        trace_bytes_used = trace_bytes_used.saturating_add(bytes_per_trace);
    }

    let mut x = n as i32;
    let mut y = m as i32;
    let mut reversed_ops: Vec<DiffOp<T>> = Vec::with_capacity(n.saturating_add(m));

    for d in (1..trace.len()).rev() {
        let v_prev = &trace[d - 1];
        let d_i32 = d as i32;
        let k = x - y;

        let prev_k = if k == -d_i32
            || (k != d_i32 && v_prev[(offset + k - 1) as usize] < v_prev[(offset + k + 1) as usize])
        {
            k + 1
        } else {
            k - 1
        };

        let prev_x = v_prev[(offset + prev_k) as usize];
        let prev_y = prev_x - prev_k;

        while x > prev_x && y > prev_y {
            reversed_ops.push(DiffOp {
                kind: DiffOpKind::Equal,
                value: before[(x - 1) as usize],
            });
            x -= 1;
            y -= 1;
        }

        if x == prev_x {
            reversed_ops.push(DiffOp {
                kind: DiffOpKind::Insert,
                value: after[prev_y as usize],
            });
        } else {
            reversed_ops.push(DiffOp {
                kind: DiffOpKind::Delete,
                value: before[prev_x as usize],
            });
        }

        x = prev_x;
        y = prev_y;
    }

    while x > 0 && y > 0 {
        reversed_ops.push(DiffOp {
            kind: DiffOpKind::Equal,
            value: before[(x - 1) as usize],
        });
        x -= 1;
        y -= 1;
    }

    while x > 0 {
        reversed_ops.push(DiffOp {
            kind: DiffOpKind::Delete,
            value: before[(x - 1) as usize],
        });
        x -= 1;
    }

    while y > 0 {
        reversed_ops.push(DiffOp {
            kind: DiffOpKind::Insert,
            value: after[(y - 1) as usize],
        });
        y -= 1;
    }

    reversed_ops.reverse();
    reversed_ops
}

fn myers_prefix_suffix_diff<T: Eq + Copy>(before: &[T], after: &[T]) -> Vec<DiffOp<T>> {
    let mut prefix = 0usize;
    while prefix < before.len() && prefix < after.len() && before[prefix] == after[prefix] {
        prefix += 1;
    }

    let mut before_end = before.len();
    let mut after_end = after.len();
    while before_end > prefix
        && after_end > prefix
        && before[before_end - 1] == after[after_end - 1]
    {
        before_end = before_end.saturating_sub(1);
        after_end = after_end.saturating_sub(1);
    }

    let mut ops: Vec<DiffOp<T>> = Vec::with_capacity(before.len().saturating_add(after.len()));
    for value in before.iter().take(prefix) {
        ops.push(DiffOp {
            kind: DiffOpKind::Equal,
            value: *value,
        });
    }

    for value in before.iter().take(before_end).skip(prefix) {
        ops.push(DiffOp {
            kind: DiffOpKind::Delete,
            value: *value,
        });
    }

    for value in after.iter().take(after_end).skip(prefix) {
        ops.push(DiffOp {
            kind: DiffOpKind::Insert,
            value: *value,
        });
    }

    for value in before.iter().skip(before_end) {
        ops.push(DiffOp {
            kind: DiffOpKind::Equal,
            value: *value,
        });
    }

    ops
}

fn split_lines_preserve_newline(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return vec![""];
    }

    let bytes = text.as_bytes();
    let mut lines = Vec::new();
    let mut start = 0usize;
    let mut index = 0usize;

    while index < bytes.len() {
        match bytes[index] {
            b'\n' => {
                lines.push(&text[start..index + 1]);
                index += 1;
                start = index;
            }
            b'\r' => {
                if index + 1 < bytes.len() && bytes[index + 1] == b'\n' {
                    lines.push(&text[start..index + 2]);
                    index += 2;
                    start = index;
                } else {
                    lines.push(&text[start..index + 1]);
                    index += 1;
                    start = index;
                }
            }
            _ => index += 1,
        }
    }

    if start < bytes.len() {
        lines.push(&text[start..]);
    }

    lines
}

fn diff_text_by_chars(before: &str, after: &str, max_trace_bytes: usize) -> Vec<DiffSpan> {
    if before == after {
        return vec![DiffSpan {
            r#type: DiffType::Unchanged,
            text: after.to_string(),
        }];
    }

    let before_chars: Vec<char> = before.chars().collect();
    let after_chars: Vec<char> = after.chars().collect();
    let ops = myers_diff(&before_chars, &after_chars, max_trace_bytes);

    let mut spans = Vec::new();
    for op in ops.into_iter() {
        match op.kind {
            DiffOpKind::Equal => push_diff(&mut spans, DiffType::Unchanged, op.value),
            DiffOpKind::Insert => push_diff(&mut spans, DiffType::Insert, op.value),
            DiffOpKind::Delete => push_diff(&mut spans, DiffType::Delete, op.value),
        }
    }
    spans
}

fn diff_text_by_lines(before: &str, after: &str, max_trace_bytes: usize) -> Vec<DiffSpan> {
    if before == after {
        return vec![DiffSpan {
            r#type: DiffType::Unchanged,
            text: after.to_string(),
        }];
    }

    const MAX_REFINED_CHARS: usize = 8_000;

    let before_lines = split_lines_preserve_newline(before);
    let after_lines = split_lines_preserve_newline(after);
    let ops = myers_diff(&before_lines, &after_lines, max_trace_bytes);

    let mut spans: Vec<DiffSpan> = Vec::new();
    let mut pending_deletes = String::new();
    let mut pending_inserts = String::new();
    fn flush_pending(
        spans: &mut Vec<DiffSpan>,
        pending_deletes: &mut String,
        pending_inserts: &mut String,
        max_refined_chars: usize,
        max_trace_bytes: usize,
    ) {
        if pending_deletes.is_empty() && pending_inserts.is_empty() {
            return;
        }

        let deleted_text = std::mem::take(pending_deletes);
        let inserted_text = std::mem::take(pending_inserts);

        if !deleted_text.is_empty() && !inserted_text.is_empty() {
            let total_chars = deleted_text
                .chars()
                .count()
                .saturating_add(inserted_text.chars().count());
            if total_chars <= max_refined_chars {
                let refined = diff_text_by_chars(&deleted_text, &inserted_text, max_trace_bytes);
                for span in refined.into_iter() {
                    push_span_text(spans, span.r#type, &span.text);
                }
                return;
            }

            push_span_text(spans, DiffType::Delete, &deleted_text);
            push_span_text(spans, DiffType::Insert, &inserted_text);
            return;
        }

        if !deleted_text.is_empty() {
            push_span_text(spans, DiffType::Delete, &deleted_text);
        }
        if !inserted_text.is_empty() {
            push_span_text(spans, DiffType::Insert, &inserted_text);
        }
    }

    for op in ops.into_iter() {
        match op.kind {
            DiffOpKind::Equal => {
                flush_pending(
                    &mut spans,
                    &mut pending_deletes,
                    &mut pending_inserts,
                    MAX_REFINED_CHARS,
                    max_trace_bytes,
                );
                push_span_text(&mut spans, DiffType::Unchanged, op.value);
            }
            DiffOpKind::Delete => pending_deletes.push_str(op.value),
            DiffOpKind::Insert => pending_inserts.push_str(op.value),
        }
    }

    flush_pending(
        &mut spans,
        &mut pending_deletes,
        &mut pending_inserts,
        MAX_REFINED_CHARS,
        max_trace_bytes,
    );
    spans
}

pub fn build_diff(source: &str, candidate: &str) -> Vec<DiffSpan> {
    const MAX_TRACE_BYTES: usize = 64 * 1024 * 1024;
    diff_text_by_lines(source, candidate, MAX_TRACE_BYTES)
}

fn push_diff(spans: &mut Vec<DiffSpan>, kind: DiffType, ch: char) {
    if let Some(last) = spans.last_mut() {
        if last.r#type == kind {
            last.text.push(ch);
            return;
        }
    }

    spans.push(DiffSpan {
        r#type: kind,
        text: ch.to_string(),
    });
}
