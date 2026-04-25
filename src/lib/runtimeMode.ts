export function isDemoRuntime() {
  return import.meta.env.MODE === "demo";
}

export function isDesktopRuntime() {
  return !isDemoRuntime();
}
