import { isDemoRuntime } from "./runtimeMode";
import { registerPickedTxtFile } from "./webFileStore";
import type {
  DialogFilter,
  OpenDialogOptions,
  SaveDialogOptions
} from "@tauri-apps/plugin-dialog";

function buildAcceptString(filters: DialogFilter[] | undefined) {
  if (!filters || filters.length === 0) {
    return ".txt,text/plain";
  }

  const accepted = new Set<string>();
  for (const filter of filters) {
    for (const extension of filter.extensions ?? []) {
      const normalized = extension.trim().toLowerCase();
      if (!normalized) continue;
      accepted.add(normalized.startsWith(".") ? normalized : `.${normalized}`);
    }
  }
  if (accepted.size === 0) {
    return ".txt,text/plain";
  }
  return Array.from(accepted).join(",");
}

async function openWebFilePicker(options: OpenDialogOptions) {
  if (typeof document === "undefined") {
    return null;
  }

  const input = document.createElement("input");
  input.type = "file";
  input.multiple = Boolean(options.multiple);
  input.accept = buildAcceptString(options.filters);

  const files = await new Promise<FileList | null>((resolve) => {
    input.onchange = () => resolve(input.files);
    input.oncancel = () => resolve(null);
    input.click();
  });

  if (!files || files.length === 0) {
    return null;
  }

  const paths: string[] = [];
  for (const file of Array.from(files)) {
    if (!file.name.toLowerCase().endsWith(".txt")) {
      continue;
    }
    const path = await registerPickedTxtFile(file);
    paths.push(path);
  }

  if (paths.length === 0) {
    return null;
  }

  if (options.multiple) {
    return paths;
  }
  return paths[0] ?? null;
}

function saveWebFileName(options: SaveDialogOptions) {
  const defaultPath = options.defaultPath?.trim() || "lessai-result.txt";
  if (typeof window === "undefined") {
    return defaultPath;
  }
  const next = window.prompt("请输入导出文件名（仅用于下载命名）", defaultPath);
  if (next == null) return null;
  const trimmed = next.trim();
  if (!trimmed) return null;
  return trimmed.toLowerCase().endsWith(".txt") ? trimmed : `${trimmed}.txt`;
}

export async function openRuntimeDialog(options: OpenDialogOptions) {
  if (isDemoRuntime()) {
    return openWebFilePicker(options);
  }
  const { open } = await import("@tauri-apps/plugin-dialog");
  return open(options);
}

export async function saveRuntimeDialog(options: SaveDialogOptions) {
  if (isDemoRuntime()) {
    return saveWebFileName(options);
  }
  const { save } = await import("@tauri-apps/plugin-dialog");
  return save(options);
}
