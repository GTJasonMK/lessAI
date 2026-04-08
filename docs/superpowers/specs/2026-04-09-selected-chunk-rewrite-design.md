# Selected Chunk Rewrite Design

## Goal
Add multi-select chunk targeting in workbench mode so users can run AI rewrite only on chosen chunks, while preserving the current manual/auto rewrite engines, concurrency, pause/resume, and review flow.

## Scope
- `Ctrl/Cmd + click` on a rewriteable chunk toggles it in the selected set.
- Plain click keeps the current single-chunk navigation behavior and clears multi-selection.
- When at least one chunk is selected, the primary run button changes from `开始优化` / `开始批处理` to `处理所选`.
- Manual mode processes only the next pending chunk inside the selected set.
- Auto mode processes only pending chunks inside the selected set and keeps using `maxConcurrency`.
- Pause, resume, and cancel continue to work for the active auto job.

## Non-Goals
- No `Shift` range select in this change.
- No batch apply/dismiss/delete in the review timeline.
- No AI chat or free-form instruction input in this change.

## UX
The left document flow remains the source of truth for chunk targeting. Active chunk and selected chunks must have different visual states. Protected chunks (`skipRewrite=true`) remain non-targetable; clicking them can still navigate, but they must not enter the selected set.

## Data Flow
Frontend keeps a `selectedChunkIndices` set for the current session. Starting rewrite passes the selected indices to the existing `start_rewrite` Tauri command. The backend treats that list as a target subset:
- manual: pick the next `idle` / `failed` chunk inside the subset
- auto: build the pending queue only from the subset
- progress: report counts for the subset being processed

## Error Handling
- Empty or fully invalid selections must not start a job.
- If all selected rewriteable chunks are already done, manual mode should surface a clear “所选片段已处理完成” style error.
- Out-of-range target indices are rejected in the backend.

## Testing
- Rust unit tests cover target-subset normalization and pending-queue selection.
- Frontend verification covers typecheck and the existing CSS regression script.
