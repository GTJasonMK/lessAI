import { useCallback, useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { readableError } from "../../lib/helpers";
import type { NoticeTone } from "../../lib/constants";

export type ResizeDirection =
  | "East"
  | "North"
  | "NorthEast"
  | "NorthWest"
  | "South"
  | "SouthEast"
  | "SouthWest"
  | "West";

type ShowNotice = (tone: NoticeTone, message: string) => void;

export function useWindowControls(showNotice: ShowNotice) {
  const [windowMaximized, setWindowMaximized] = useState(false);

  useEffect(() => {
    void (async () => {
      try {
        const maximized = await getCurrentWindow().isMaximized();
        setWindowMaximized(maximized);
      } catch {
        // 在非 Tauri 环境（或权限受限）下忽略即可。
      }
    })();
  }, []);

  const handleMinimizeWindow = useCallback(async () => {
    try {
      await getCurrentWindow().minimize();
    } catch (error) {
      showNotice("error", `窗口最小化失败：${readableError(error)}`);
    }
  }, [showNotice]);

  const handleToggleMaximizeWindow = useCallback(async () => {
    try {
      const appWindow = getCurrentWindow();
      await appWindow.toggleMaximize();
      const maximized = await appWindow.isMaximized();
      setWindowMaximized(maximized);
    } catch (error) {
      showNotice("error", `窗口最大化切换失败：${readableError(error)}`);
    }
  }, [showNotice]);

  const handleCloseWindow = useCallback(async () => {
    try {
      await getCurrentWindow().close();
    } catch (error) {
      showNotice("error", `窗口关闭失败：${readableError(error)}`);
    }
  }, [showNotice]);

  // 无边框窗口缩放：用边缘热区触发 `startResizeDragging`
  const handleResizeWindow = useCallback(
    async (direction: ResizeDirection) => {
      try {
        await getCurrentWindow().startResizeDragging(direction);
      } catch (error) {
        showNotice("error", `窗口缩放失败：${readableError(error)}`);
      }
    },
    [showNotice]
  );

  return {
    windowMaximized,
    handleMinimizeWindow,
    handleToggleMaximizeWindow,
    handleCloseWindow,
    handleResizeWindow
  } as const;
}

