export function fileExtensionLower(path: string) {
  const value = path.trim();
  if (!value) return "";

  const lastSlash = Math.max(value.lastIndexOf("/"), value.lastIndexOf("\\"));
  const base = lastSlash >= 0 ? value.slice(lastSlash + 1) : value;
  const dot = base.lastIndexOf(".");
  if (dot <= 0) return "";
  return base.slice(dot + 1).toLowerCase();
}
