import { useCallback } from "react";
import { relaunch } from "@tauri-apps/plugin-process";
import { check } from "@tauri-apps/plugin-updater";
import { BundleType, getBundleType, getVersion } from "@tauri-apps/api/app";
import { formatBytes, readableError } from "../../lib/helpers";
import type { NoticeTone } from "../../lib/constants";
import type { ConfirmModalOptions } from "../../components/ConfirmModal";

const UPDATE_MANIFEST_URL =
  "https://github.com/GTJasonMK/lessAI/releases/latest/download/latest.json";

type ShowNotice = (
  tone: NoticeTone,
  message: string,
  options?: { autoDismissMs?: number | null }
) => void;

export function useUpdateChecker(options: {
  updateProxy: string;
  showNotice: ShowNotice;
  dismissNotice: () => void;
  requestConfirm: (options: ConfirmModalOptions) => Promise<boolean>;
  withBusy: <T>(action: string, fn: () => Promise<T>) => Promise<T>;
}) {
  const { updateProxy, showNotice, dismissNotice, requestConfirm, withBusy } = options;

  const handleCheckUpdate = useCallback(async () => {
    try {
      if (import.meta.env.DEV) {
        showNotice(
          "warning",
          [
            "你正在通过开发模式启动（start-lessai.bat / tauri dev）。",
            "应用内更新只对“已安装的 Release 版本”生效，不会覆盖当前源码运行实例。",
            "想升级源码：请 git 拉取最新 tag/分支后重新运行；想升级安装版：请从开始菜单启动已安装的 LessAI 再检查更新。"
          ].join("\n"),
          { autoDismissMs: 12_000 }
        );
        return;
      }

      const currentVersion = await getVersion();
      const bundleType = await getBundleType();

      if (bundleType === BundleType.Deb || bundleType === BundleType.Rpm) {
        showNotice(
          "warning",
          `当前安装包类型（${bundleType}）不支持应用内更新，请前往 GitHub Releases 下载新版本。`
        );
        return;
      }

      await withBusy("check-update", async () => {
        showNotice("info", "正在检查更新…", { autoDismissMs: null });

        const rawProxy = updateProxy.trim();
        const proxy =
          rawProxy && !rawProxy.includes("://") ? `http://${rawProxy}` : rawProxy || undefined;

        const update = await check({ timeout: 15_000, proxy });
        if (!update) {
          showNotice("success", `已是最新版本（${currentVersion}）。`);
          return;
        }

        // 发现更新后进入确认弹窗，先收起“检查中”提示，避免干扰阅读。
        dismissNotice();

        const messageParts = [
          `当前版本：${currentVersion}`,
          `发现新版本：${update.version}`,
          update.date ? `发布时间：${update.date}` : null,
          update.body?.trim() ? `更新内容：\n${update.body.trim()}` : null,
          "",
          "是否立即下载并安装？"
        ].filter((item): item is string => Boolean(item));

        const ok = await requestConfirm({
          title: "发现新版本",
          message: messageParts.join("\n"),
          okLabel: "立即更新",
          cancelLabel: "稍后"
        });

        if (!ok) {
          await update.close();
          return;
        }

        let contentLength: number | null = null;
        let downloadedBytes = 0;
        let lastNoticeAt = 0;

        const pushDownloadNotice = (force = false) => {
          const now = Date.now();
          if (!force && now - lastNoticeAt < 120) return;
          lastNoticeAt = now;

          const totalBytes = contentLength ?? 0;
          const hasTotal = totalBytes > 0;
          const percent = hasTotal
            ? Math.max(0, Math.min(100, Math.floor((downloadedBytes / totalBytes) * 100)))
            : null;

          const progressText = hasTotal
            ? `${percent}%（${formatBytes(downloadedBytes)} / ${formatBytes(totalBytes)}）`
            : `已下载 ${formatBytes(downloadedBytes)}`;

          showNotice("info", `正在下载更新… ${progressText}`, { autoDismissMs: null });
        };

        pushDownloadNotice(true);

        try {
          await update.downloadAndInstall((event) => {
            switch (event.event) {
              case "Started":
                contentLength = event.data.contentLength ?? null;
                downloadedBytes = 0;
                pushDownloadNotice(true);
                break;
              case "Progress":
                downloadedBytes += event.data.chunkLength;
                pushDownloadNotice(false);
                break;
              case "Finished":
                showNotice("info", "下载完成，正在安装更新…", { autoDismissMs: null });
                break;
              default:
                break;
            }
          });
        } finally {
          try {
            await update.close();
          } catch {
            // ignore
          }
        }

        // 注意：Windows 平台由于系统限制，安装程序执行时应用可能会直接退出。
        // 其他平台安装完成后可调用 relaunch() 自动重启。
        try {
          showNotice("success", "更新已安装，正在重启应用…", { autoDismissMs: null });
          await relaunch();
        } catch (error) {
          showNotice("warning", `更新已安装，请手动重启应用：${readableError(error)}`);
        }
      });
    } catch (error) {
      const message = readableError(error);

      if (
        message.includes("Could not fetch a valid release JSON") ||
        /valid release json/i.test(message)
      ) {
        showNotice(
          "error",
          [
            "检查更新失败：无法从更新源拿到有效响应（GitHub 返回非 2xx）。",
            `更新源：${UPDATE_MANIFEST_URL}`,
            "如果浏览器能打开但应用内失败：通常是网络/代理差异，可在设置里填写“更新代理”（例如 http://127.0.0.1:7890）后重试。",
            "如果浏览器打开需要登录或是 404：说明 Release 资源未公开或 latest.json 尚未生成/上传。",
            `原始错误：${message}`
          ].join("\n"),
          { autoDismissMs: 12_000 }
        );
        return;
      }

      showNotice(
        "error",
        `检查更新失败：${message}${
          /updater|pubkey|endpoint|permission/i.test(message)
            ? "\n（提示：需要在 tauri.conf.json 配置 updater.endpoints/pubkey，并在 capabilities 授权 updater:default；Release 构建需合并 tauri.updater.conf.json 以生成签名产物）"
            : ""
        }`
      );
    }
  }, [dismissNotice, requestConfirm, showNotice, updateProxy, withBusy]);

  return { handleCheckUpdate } as const;
}

