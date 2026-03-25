import { useCallback, useRef, useState } from "react";
import type { ConfirmModalOptions } from "../../components/ConfirmModal";

export function useConfirmDialog() {
  const [confirmDialog, setConfirmDialog] = useState<ConfirmModalOptions | null>(null);
  const confirmResolverRef = useRef<((value: boolean) => void) | null>(null);

  const requestConfirm = useCallback((options: ConfirmModalOptions) => {
    return new Promise<boolean>((resolve) => {
      // 若外部逻辑同时触发多次确认弹窗，后者覆盖前者；前者默认视为取消。
      if (confirmResolverRef.current) {
        confirmResolverRef.current(false);
      }
      confirmResolverRef.current = resolve;
      setConfirmDialog(options);
    });
  }, []);

  const handleConfirmResult = useCallback((value: boolean) => {
    const resolve = confirmResolverRef.current;
    confirmResolverRef.current = null;
    setConfirmDialog(null);
    resolve?.(value);
  }, []);

  return { confirmDialog, requestConfirm, handleConfirmResult } as const;
}

