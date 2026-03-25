import { useCallback, useMemo } from "react";
import type { DocumentSession } from "../../lib/types";

type Stage = "workbench" | "editor";

export interface ChunkStrategyLockState {
  locked: boolean;
  reason: string;
}

function computeChunkStrategyLockState(
  stage: Stage,
  editorDirty: boolean,
  session: DocumentSession | null
): ChunkStrategyLockState {
  // 切段策略（chunkPreset + rewriteHeadings）属于“项目级配置”：
  // - 项目一旦产生修改对/进度，切段策略必须保持稳定，否则最小审阅单元（chunk）会失去一致性；
  // - 需要调整时，应先回到“空项目”（无修改对/无进度）的边界，再修改策略，并通过重置/新导入生效。
  if (stage === "editor") {
    return {
      locked: true,
      reason: editorDirty
        ? "当前有未保存的手动编辑。为保证审阅单元稳定，请先保存或放弃修改后再调整切段策略。"
        : "编辑模式下切段策略已锁定。请先返回工作台后再调整。"
    };
  }

  if (!session) {
    return { locked: false, reason: "" };
  }

  if (session.status === "running" || session.status === "paused") {
    return {
      locked: true,
      reason: "文档正在执行自动任务，切段策略已锁定。请先取消任务或等待完成。"
    };
  }

  if (session.suggestions.length > 0) {
    return {
      locked: true,
      reason:
        "当前项目已有修改对记录，切段策略已锁定。若需调整，请先“重置记录”（会清空修改对/进度）。"
    };
  }

  const hasProgress = session.chunks.some(
    (chunk) => !chunk.skipRewrite && chunk.status !== "idle"
  );
  if (hasProgress) {
    return {
      locked: true,
      reason:
        "当前项目已有生成进度/失败片段，切段策略已锁定。若需调整，请先“重置记录”。"
    };
  }

  return { locked: false, reason: "" };
}

export function useChunkStrategyLock(options: {
  stage: Stage;
  editorDirty: boolean;
  currentSession: DocumentSession | null;
  stageRef: React.RefObject<Stage>;
  editorDirtyRef: React.RefObject<boolean>;
  currentSessionRef: React.RefObject<DocumentSession | null>;
}) {
  const { stage, editorDirty, currentSession, stageRef, editorDirtyRef, currentSessionRef } =
    options;

  const chunkStrategyLock = useMemo(
    () => computeChunkStrategyLockState(stage, editorDirty, currentSession),
    [currentSession, editorDirty, stage]
  );

  const readChunkStrategyLockedReason = useCallback(() => {
    const { locked, reason } = computeChunkStrategyLockState(
      stageRef.current,
      editorDirtyRef.current,
      currentSessionRef.current
    );
    return locked ? reason : null;
  }, [currentSessionRef, editorDirtyRef, stageRef]);

  return { chunkStrategyLock, readChunkStrategyLockedReason } as const;
}

