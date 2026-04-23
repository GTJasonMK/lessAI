import { assertIncludes, read } from "./test-helpers.mjs";

const appSource = read("src/App.tsx");
const sessionActionShared = read("src/app/hooks/sessionActionShared.ts");
const rewriteActions = read("src/app/hooks/useRewriteActions.ts");
const suggestionActions = read("src/app/hooks/useSuggestionActions.ts");

assertIncludes(sessionActionShared, "preserveScroll?: boolean;");
assertIncludes(sessionActionShared, "preservedScrollTop?: number | null");

assertIncludes(appSource, "options?.preserveScroll === false ? undefined : captureDocumentScrollPosition()");
assertIncludes(appSource, "options.preservedScrollTop ?? null");

assertIncludes(rewriteActions, "captureDocumentScrollPosition: () => number | null;");
assertIncludes(rewriteActions, "runSessionActionOrNotify({");
assertIncludes(rewriteActions, "captureDocumentScrollPosition,");

assertIncludes(suggestionActions, "captureDocumentScrollPosition: () => number | null;");
assertIncludes(suggestionActions, "runSessionActionOrNotify({");
assertIncludes(suggestionActions, "captureDocumentScrollPosition,");

console.log("[workbench-scroll-regression] OK");
