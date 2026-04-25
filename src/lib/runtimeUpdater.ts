import { isDemoRuntime } from "./runtimeMode";
import type { DownloadEvent } from "@tauri-apps/plugin-updater";

export const RuntimeBundleType = {
  Deb: "Deb",
  Rpm: "Rpm",
  AppImage: "AppImage",
  Other: "Other"
} as const;

export type RuntimeBundleTypeValue =
  typeof RuntimeBundleType[keyof typeof RuntimeBundleType];

export interface RuntimeUpdate {
  version: string;
  date?: string | null;
  body?: string | null;
  close: () => Promise<void>;
  downloadAndInstall: (
    onEvent: (event: DownloadEvent) => void
  ) => Promise<void>;
}

export async function runtimeGetVersion() {
  if (isDemoRuntime()) {
    return "web-demo";
  }
  const { getVersion } = await import("@tauri-apps/api/app");
  return getVersion();
}

export async function runtimeGetBundleType(): Promise<RuntimeBundleTypeValue> {
  if (isDemoRuntime()) {
    return RuntimeBundleType.Other;
  }

  const { BundleType, getBundleType } = await import("@tauri-apps/api/app");
  const type = await getBundleType();
  if (type === BundleType.Deb) return RuntimeBundleType.Deb;
  if (type === BundleType.Rpm) return RuntimeBundleType.Rpm;
  if (type === BundleType.AppImage) return RuntimeBundleType.AppImage;
  return RuntimeBundleType.Other;
}

export async function runtimeCheckUpdate(options: {
  timeout: number;
  proxy?: string;
}): Promise<RuntimeUpdate | null> {
  if (isDemoRuntime()) {
    return null;
  }

  const { check } = await import("@tauri-apps/plugin-updater");
  const update = await check({ timeout: options.timeout, proxy: options.proxy });
  if (!update) {
    return null;
  }

  return {
    version: String(update.version),
    date: update.date,
    body: update.body,
    close: () => update.close(),
    downloadAndInstall: (onEvent) => update.downloadAndInstall(onEvent)
  };
}

export async function runtimeRelaunch() {
  if (isDemoRuntime()) {
    if (typeof window !== "undefined") {
      window.location.reload();
      return;
    }
    return;
  }

  const { relaunch } = await import("@tauri-apps/plugin-process");
  await relaunch();
}
