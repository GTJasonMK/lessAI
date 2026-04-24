import { useCallback, useEffect, useMemo, useState } from "react";
import {
  closeMainWindow,
  isMainWindowMaximized,
  minimizeMainWindow,
  startDragMainWindow,
  startResizeMainWindow,
  toggleMaximizeMainWindow,
  type WindowResizeDirection
} from "../../lib/api";
import { readableError } from "../../lib/helpers";
import type { ShowNotice } from "./sessionActionShared";

export type ResizeDirection = WindowResizeDirection;

function isLinuxDesktop() {
  if (typeof navigator === "undefined") {
    return false;
  }

  const navigatorWithUAData = navigator as Navigator & {
    userAgentData?: { platform?: string };
  };
  const platform = navigatorWithUAData.userAgentData?.platform ?? navigator.platform ?? "";
  const userAgent = navigator.userAgent ?? "";

  return /linux/i.test(`${platform} ${userAgent}`) && !/android/i.test(userAgent);
}

export function useWindowControls(showNotice: ShowNotice) {
  const [windowMaximized, setWindowMaximized] = useState(false);
  const customResizeEnabled = useMemo(() => !isLinuxDesktop(), []);

  useEffect(() => {
    let disposed = false;

    const syncWindowMaximized = async () => {
      try {
        const maximized = await isMainWindowMaximized();
        if (!disposed) {
          setWindowMaximized(maximized);
        }
      } catch {
        // 在非 Tauri 环境（或权限受限）下忽略即可。
      }
    };

    const handleResize = () => {
      void syncWindowMaximized();
    };

    void syncWindowMaximized();
    window.addEventListener("resize", handleResize);
    window.addEventListener("focus", handleResize);

    return () => {
      disposed = true;
      window.removeEventListener("resize", handleResize);
      window.removeEventListener("focus", handleResize);
    };
  }, []);

  const handleMinimizeWindow = useCallback(async () => {
    try {
      await minimizeMainWindow();
    } catch (error) {
      showNotice("error", `窗口最小化失败：${readableError(error)}`);
    }
  }, [showNotice]);

  const handleToggleMaximizeWindow = useCallback(async () => {
    try {
      await toggleMaximizeMainWindow();
      const maximized = await isMainWindowMaximized();
      setWindowMaximized(maximized);
    } catch (error) {
      showNotice("error", `窗口最大化切换失败：${readableError(error)}`);
    }
  }, [showNotice]);

  const handleCloseWindow = useCallback(async () => {
    try {
      await closeMainWindow();
    } catch (error) {
      showNotice("error", `窗口关闭失败：${readableError(error)}`);
    }
  }, [showNotice]);

  const handleStartWindowDrag = useCallback(async () => {
    try {
      await startDragMainWindow();
    } catch (error) {
      showNotice("error", `窗口拖动失败：${readableError(error)}`);
    }
  }, [showNotice]);

  const handleResizeWindow = useCallback(
    async (direction: ResizeDirection) => {
      try {
        await startResizeMainWindow(direction);
      } catch (error) {
        showNotice("error", `窗口缩放失败：${readableError(error)}`);
      }
    },
    [showNotice]
  );

  return {
    customResizeEnabled,
    windowMaximized,
    handleStartWindowDrag,
    handleMinimizeWindow,
    handleToggleMaximizeWindow,
    handleCloseWindow,
    handleResizeWindow
  } as const;
}
