import { assertIncludes, read } from "./test-helpers.mjs";

const mainRs = read("src-tauri/src/main.rs");
const debugLogger = read("src/app/hooks/documentScrollRestoreDebug.ts");

assertIncludes(mainRs, "TargetKind::LogDir");
assertIncludes(mainRs, "TargetKind::Stdout");
assertIncludes(mainRs, "TargetKind::Webview");
assertIncludes(debugLogger, 'import { info } from "@tauri-apps/plugin-log";');
assertIncludes(debugLogger, "void info(");
assertIncludes(debugLogger, "JSON.stringify(detail)");

console.log("[scroll-log-persistence] OK");
