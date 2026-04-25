import { isDemoRuntime } from "./runtimeMode";

export async function attachRuntimeConsole() {
  if (isDemoRuntime()) {
    return;
  }
  const { attachConsole } = await import("@tauri-apps/plugin-log");
  await attachConsole();
}

export async function infoRuntime(message: string) {
  if (isDemoRuntime()) {
    console.info(message);
    return;
  }
  const { info } = await import("@tauri-apps/plugin-log");
  await info(message);
}
