interface VirtualFileEntry {
  path: string;
  name: string;
  title: string;
  text: string;
  fingerprint: string;
  updatedAt: string;
}

const filesByPath = new Map<string, VirtualFileEntry>();
const pathByFingerprint = new Map<string, string>();

function simpleHash(input: string) {
  let hash = 0;
  for (let index = 0; index < input.length; index += 1) {
    hash = (hash << 5) - hash + input.charCodeAt(index);
    hash |= 0;
  }
  return Math.abs(hash).toString(16);
}

function fileTitleFromName(name: string) {
  const trimmed = name.trim();
  if (!trimmed) return "untitled";
  const index = trimmed.lastIndexOf(".");
  if (index <= 0) return trimmed;
  return trimmed.slice(0, index);
}

function buildFingerprint(file: File) {
  return `${file.name}:${file.size}:${file.lastModified}`;
}

function buildPath(file: File, fingerprint: string) {
  const encodedName = encodeURIComponent(file.name || "untitled.txt");
  return `web://txt/${encodedName}?id=${simpleHash(fingerprint)}`;
}

export async function registerPickedTxtFile(file: File) {
  const fingerprint = buildFingerprint(file);
  const existingPath = pathByFingerprint.get(fingerprint);
  const text = await file.text();

  if (existingPath) {
    const previous = filesByPath.get(existingPath);
    if (previous) {
      previous.text = text;
      previous.updatedAt = new Date().toISOString();
      return existingPath;
    }
  }

  const path = buildPath(file, fingerprint);
  const entry: VirtualFileEntry = {
    path,
    name: file.name,
    title: fileTitleFromName(file.name),
    text,
    fingerprint,
    updatedAt: new Date().toISOString()
  };
  filesByPath.set(path, entry);
  pathByFingerprint.set(fingerprint, path);
  return path;
}

export function getVirtualFile(path: string) {
  return filesByPath.get(path) ?? null;
}

export function updateVirtualFileText(path: string, nextText: string) {
  const file = filesByPath.get(path);
  if (!file) {
    throw new Error("未找到对应的网页文档缓存。");
  }
  file.text = nextText;
  file.updatedAt = new Date().toISOString();
}
