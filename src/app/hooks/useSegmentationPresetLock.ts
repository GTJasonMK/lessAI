import { useCallback, useMemo } from "react";
import type { DocumentSession } from "../../lib/types";
import { rewriteUnitHasEditableSlot } from "../../lib/helpers";

type Stage = "workbench" | "editor";

export interface SegmentationPresetLockState {
  locked: boolean;
  reason: string;
}

function computeSegmentationPresetLockState(
  stage: Stage,
  editorDirty: boolean,
  session: DocumentSession | null
): SegmentationPresetLockState {
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

  const hasProgress = session.rewriteUnits.some(
    (rewriteUnit) =>
      rewriteUnitHasEditableSlot(session, rewriteUnit) && rewriteUnit.status !== "idle"
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

export function useSegmentationPresetLock(options: {
  stage: Stage;
  editorDirty: boolean;
  currentSession: DocumentSession | null;
  stageRef: React.RefObject<Stage>;
  editorDirtyRef: React.RefObject<boolean>;
  currentSessionRef: React.RefObject<DocumentSession | null>;
}) {
  const { stage, editorDirty, currentSession, stageRef, editorDirtyRef, currentSessionRef } =
    options;

  const segmentationPresetLock = useMemo(
    () => computeSegmentationPresetLockState(stage, editorDirty, currentSession),
    [currentSession, editorDirty, stage]
  );

  const readSegmentationPresetLockedReason = useCallback(() => {
    const { locked, reason } = computeSegmentationPresetLockState(
      stageRef.current,
      editorDirtyRef.current,
      currentSessionRef.current
    );
    return locked ? reason : null;
  }, [currentSessionRef, editorDirtyRef, stageRef]);

  return { segmentationPresetLock, readSegmentationPresetLockedReason } as const;
}
