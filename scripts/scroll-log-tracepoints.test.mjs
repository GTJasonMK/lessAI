import { assertIncludes, read } from "./test-helpers.mjs";

const appSource = read("src/App.tsx");
const paragraphFlow = read("src/stages/workbench/document/ParagraphDocumentFlow.tsx");

assertIncludes(appSource, 'logScrollRestore("refresh-session-state-start"');
assertIncludes(appSource, 'logScrollRestore("refresh-session-state-loaded"');
assertIncludes(appSource, 'logScrollRestore("tauri-rewrite-unit-completed"');
assertIncludes(appSource, 'logScrollRestore("tauri-finished"');
assertIncludes(paragraphFlow, 'logScrollRestore("paragraph-scroll-into-view"');

console.log("[scroll-log-tracepoints] OK");
