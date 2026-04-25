import { assertIncludes, read } from "./test-helpers.mjs";

const mainRs = read("src-tauri/src/main.rs");
const debugLogger = read("src/app/hooks/documentScrollRestoreDebug.ts");
const runtimeLog = read("src/lib/runtimeLog.ts");

assertIncludes(mainRs, "TargetKind::LogDir");
assertIncludes(mainRs, "TargetKind::Stdout");
assertIncludes(mainRs, "TargetKind::Webview");
assertIncludes(debugLogger, 'import { infoRuntime } from "../../lib/runtimeLog";');
assertIncludes(debugLogger, "void infoRuntime(");
assertIncludes(debugLogger, "JSON.stringify(detail)");
assertIncludes(runtimeLog, 'await import("@tauri-apps/plugin-log")');
assertIncludes(runtimeLog, "await info(message);");

console.log("[scroll-log-persistence] OK");
