interface ScriptStats {
  cjk: number;
  latin: number;
  digits: number;
  total: number;
}

export function isCjkChar(char: string) {
  const code = char.codePointAt(0);
  if (code == null) {
    return false;
  }
  return (
    (code >= 0x3400 && code <= 0x4dbf) ||
    (code >= 0x4e00 && code <= 0x9fff) ||
    (code >= 0xf900 && code <= 0xfaff)
  );
}

export function scriptStats(text: string): ScriptStats {
  const stats: ScriptStats = { cjk: 0, latin: 0, digits: 0, total: 0 };
  for (const char of text) {
    if (/\s/.test(char)) {
      continue;
    }
    if (isCjkChar(char)) {
      stats.cjk += 1;
      stats.total += 1;
      continue;
    }
    if (/[A-Za-z]/.test(char)) {
      stats.latin += 1;
      stats.total += 1;
      continue;
    }
    if (/\d/.test(char)) {
      stats.digits += 1;
      stats.total += 1;
    }
  }
  return stats;
}

export function isMostlyCjk(stats: ScriptStats) {
  return stats.total >= 20 && Math.floor((stats.cjk * 100) / stats.total) >= 40;
}

export function isMostlyLatin(stats: ScriptStats) {
  return stats.total >= 20 && Math.floor((stats.latin * 100) / stats.total) >= 60;
}

export function findUnwantedMetaPattern(text: string) {
  const metaPatternsEn = [
    "i am claude",
    "made by anthropic",
    "helpful, harmless, and honest",
    "i'm an ai assistant",
    "i am an ai assistant",
    "as an ai language model",
    "as an ai assistant",
    "happy to help you",
    "i don't have information about the specific model version",
    "i don't have information about the specific model version or id"
  ];
  const lowered = text.toLowerCase();
  for (const pattern of metaPatternsEn) {
    if (lowered.includes(pattern)) {
      return pattern;
    }
  }

  const metaPatternsZh = [
    "我是一个ai助手",
    "我是一名ai助手",
    "作为一个ai助手",
    "作为一名ai助手",
    "作为ai语言模型",
    "作为一个ai语言模型",
    "作为一名ai语言模型",
    "作为一个人工智能助手",
    "我无法访问",
    "我不能访问"
  ];
  for (const pattern of metaPatternsZh) {
    if (text.includes(pattern)) {
      return pattern;
    }
  }
  return null;
}

export function isBoundaryAfterPrefix(rest: string) {
  if (!rest) {
    return true;
  }
  const first = Array.from(rest)[0] ?? "";
  return (
    /\s/.test(first) ||
    ["，", ",", "。", ".", "！", "!", "？", "?", ":", "：", ";", "；"].includes(first)
  );
}

export function startsWithPhrase(text: string, phrase: string) {
  if (!text.startsWith(phrase)) {
    return false;
  }
  return isBoundaryAfterPrefix(text.slice(phrase.length));
}

export function startsWithAnyPhrase(text: string, phrases: string[]) {
  for (const phrase of phrases) {
    if (startsWithPhrase(text, phrase)) {
      return phrase;
    }
  }
  return null;
}

export function startsWithAnyPrefix(text: string, prefixes: string[]) {
  for (const prefix of prefixes) {
    if (text.startsWith(prefix)) {
      return prefix;
    }
  }
  return null;
}

export function firstNonEmptyLine(text: string) {
  for (const line of text.split(/\r\n|\r|\n/)) {
    if (line.trim()) {
      return trimAsciiSpacesTabsStart(line);
    }
  }
  return trimAsciiSpacesTabsStart(text);
}

export function trimAsciiSpacesTabsStart(text: string) {
  let index = 0;
  while (index < text.length && (text[index] === " " || text[index] === "\t")) {
    index += 1;
  }
  return text.slice(index);
}

export function trimAsciiSpacesTabsEnd(text: string) {
  let end = text.length;
  while (end > 0 && (text[end - 1] === " " || text[end - 1] === "\t")) {
    end -= 1;
  }
  return text.slice(0, end);
}

export function trimAsciiSpacesTabs(text: string) {
  return trimAsciiSpacesTabsStart(trimAsciiSpacesTabsEnd(text));
}

export function detectLineMarkerLen(rest: string) {
  if (!rest) {
    return 0;
  }
  const first = Array.from(rest)[0] ?? "";

  if (first === "#") {
    let end = first.length;
    for (let index = first.length; index < rest.length; index += 1) {
      if (rest[index] === "#") {
        end = index + 1;
      } else {
        break;
      }
    }
    return end;
  }

  if (first === ">") {
    let end = first.length;
    for (let index = first.length; index < rest.length; index += 1) {
      if (rest[index] === ">") {
        end = index + 1;
      } else {
        break;
      }
    }
    return end;
  }

  if (["-", "*", "+", "•", "·"].includes(first)) {
    return first.length;
  }

  if (/\d/.test(first)) {
    let digitsEnd = first.length;
    for (let index = first.length; index < rest.length; index += 1) {
      if (/\d/.test(rest[index] ?? "")) {
        digitsEnd = index + 1;
      } else {
        break;
      }
    }

    const after = rest.slice(digitsEnd);
    const marker = Array.from(after)[0] ?? "";
    if ([".", "．", ")", "）", "、"].includes(marker)) {
      return digitsEnd + marker.length;
    }
    return 0;
  }

  if (first === "(" || first === "（") {
    const closing = first === "(" ? ")" : "）";
    let count = 0;
    for (let index = first.length; index < rest.length; index += 1) {
      count += 1;
      if (count > 12) {
        break;
      }
      if (rest[index] === closing) {
        return index + 1;
      }
    }
  }

  return 0;
}

export function splitLineSkeleton(line: string) {
  const base = trimAsciiSpacesTabsEnd(line);
  const suffix = line.slice(base.length);
  let indentEnd = 0;
  while (
    indentEnd < base.length &&
    (base[indentEnd] === " " || base[indentEnd] === "\t")
  ) {
    indentEnd += 1;
  }

  const rest = base.slice(indentEnd);
  const markerLen = detectLineMarkerLen(rest);
  let prefixEnd = indentEnd + markerLen;
  while (
    prefixEnd < base.length &&
    (base[prefixEnd] === " " || base[prefixEnd] === "\t")
  ) {
    prefixEnd += 1;
  }

  return {
    prefix: base.slice(0, prefixEnd),
    core: base.slice(prefixEnd),
    suffix
  };
}

export function stripRedundantPrefix(candidate: string, sourcePrefix: string) {
  let body = trimAsciiSpacesTabs(candidate);
  const marker = trimAsciiSpacesTabsStart(sourcePrefix);
  if (marker && body.startsWith(marker)) {
    body = trimAsciiSpacesTabs(body.slice(marker.length));
  }
  return body;
}

export function enforceLineSkeleton(sourceLine: string, candidateLine: string) {
  if (!sourceLine.trim()) {
    return sourceLine;
  }
  const { prefix, core, suffix } = splitLineSkeleton(sourceLine);
  if (!core.trim()) {
    return sourceLine;
  }
  const body = stripRedundantPrefix(candidateLine, prefix);
  if (!body.trim()) {
    return sourceLine;
  }
  return `${prefix}${body}${suffix}`;
}

export function normalizeLineForPrefaceDetection(line: string) {
  const trimmed = trimAsciiSpacesTabsStart(line);
  const { prefix, core } = splitLineSkeleton(trimmed);
  const tail = prefix[prefix.length - 1] ?? "";
  const prefixLooksStructural = tail === " " || tail === "\t";
  if (prefixLooksStructural && core.trim()) {
    return trimAsciiSpacesTabsStart(core);
  }
  return trimmed;
}

export function findUnwantedPreface(source: string, candidate: string) {
  const sourceLineRaw = firstNonEmptyLine(source.trimStart());
  const candidateLineRaw = firstNonEmptyLine(candidate.trimStart());
  const sourceLine = normalizeLineForPrefaceDetection(sourceLineRaw);
  const candidateLine = normalizeLineForPrefaceDetection(candidateLineRaw);

  const greetingsZh = ["你好", "您好", "嗨", "哈喽", "早上好", "下午好", "晚上好"];
  const sourceHasGreeting = Boolean(startsWithAnyPhrase(sourceLine, greetingsZh));
  if (startsWithAnyPhrase(candidateLine, greetingsZh) && !sourceHasGreeting) {
    return "问候语";
  }

  const sourceLower = sourceLine.toLowerCase();
  const candidateLower = candidateLine.toLowerCase();
  const greetingsEn = ["hi", "hello", "hey"];
  const sourceHasGreetingEn = Boolean(startsWithAnyPhrase(sourceLower, greetingsEn));
  if (startsWithAnyPhrase(candidateLower, greetingsEn) && !sourceHasGreetingEn) {
    return "greeting";
  }

  const prefaceZh = ["当然可以", "没问题", "好的", "可以的"];
  const sourceHasPreface = Boolean(startsWithAnyPhrase(sourceLine, prefaceZh));
  const matchedPreface = startsWithAnyPhrase(candidateLine, prefaceZh);
  if (matchedPreface && !sourceHasPreface) {
    return matchedPreface;
  }

  const prefaceRewritePrefix = ["下面是", "以下是", "这里是"];
  const metaHint =
    candidateLine.includes("改写") ||
    candidateLine.includes("润色") ||
    candidateLine.includes("降重") ||
    candidateLine.includes("优化");
  if (
    metaHint &&
    !startsWithAnyPrefix(sourceLine, prefaceRewritePrefix) &&
    startsWithAnyPrefix(candidateLine, prefaceRewritePrefix)
  ) {
    return "改写引导语";
  }

  return null;
}

export function findUnwantedRewriteLabel(source: string, candidate: string) {
  const sourceLineRaw = firstNonEmptyLine(source.trimStart());
  const candidateLineRaw = firstNonEmptyLine(candidate.trimStart());
  const sourceLine = normalizeLineForPrefaceDetection(sourceLineRaw);
  const candidateLine = normalizeLineForPrefaceDetection(candidateLineRaw);

  const labels = ["修改后", "改写后", "润色后"];
  const sourceHasLabel = Boolean(startsWithAnyPhrase(sourceLine, labels));
  const candidateLabel = startsWithAnyPhrase(candidateLine, labels);
  if (!sourceHasLabel) {
    return candidateLabel;
  }
  return null;
}

export function validateSelectionRewriteOutput(source: string, candidate: string) {
  if (!candidate.trim()) {
    throw new Error("模型输出为空。");
  }
  if (candidate.trimStart().startsWith("```")) {
    throw new Error("模型输出包含代码块围栏。");
  }

  const metaPattern = findUnwantedMetaPattern(candidate);
  if (metaPattern) {
    const sourceLower = source.toLowerCase();
    const candidateLower = candidate.toLowerCase();
    const inSource = source.includes(metaPattern) || sourceLower.includes(metaPattern);
    const inCandidate =
      candidate.includes(metaPattern) || candidateLower.includes(metaPattern);
    if (inCandidate && !inSource) {
      throw new Error(`模型输出疑似自我介绍/免责声明（命中：${metaPattern}）。`);
    }
  }

  const preface = findUnwantedPreface(source, candidate);
  if (preface) {
    throw new Error(`模型输出疑似客套/问候开场（命中：${preface}）。`);
  }

  const rewriteLabel = findUnwantedRewriteLabel(source, candidate);
  if (rewriteLabel) {
    throw new Error(`模型输出包含额外改写标签（命中：${rewriteLabel}）。`);
  }

  const sourceStats = scriptStats(source);
  const candidateStats = scriptStats(candidate);
  if (
    (isMostlyCjk(sourceStats) && isMostlyLatin(candidateStats)) ||
    (isMostlyLatin(sourceStats) && isMostlyCjk(candidateStats))
  ) {
    throw new Error("模型输出语言与原文不匹配（疑似跑题）。");
  }
}

export function normalizeLineEndingsToLf(text: string) {
  return text.replace(/\r\n/g, "\n").replace(/\r/g, "\n");
}

export function splitLinesKeepEmpty(text: string) {
  return normalizeLineEndingsToLf(text).split("\n");
}

export function blankPatternMatches(sourceLines: string[], candidateLines: string[]) {
  if (sourceLines.length !== candidateLines.length) {
    return false;
  }
  for (let index = 0; index < sourceLines.length; index += 1) {
    if (
      (sourceLines[index].trim() === "") !==
      (candidateLines[index].trim() === "")
    ) {
      return false;
    }
  }
  return true;
}

export function tryParseMultilineRewriteResponse(output: string, expectedLines: number) {
  const normalized = normalizeLineEndingsToLf(output);
  const collected: Array<string | null> = new Array(expectedLines).fill(null);

  for (const rawLine of normalized.split("\n")) {
    if (!rawLine.startsWith("@@@")) {
      continue;
    }
    const rest = rawLine.slice(3);
    let digitsEnd = 0;
    while (digitsEnd < rest.length && /\d/.test(rest[digitsEnd] ?? "")) {
      digitsEnd += 1;
    }
    if (digitsEnd === 0) {
      continue;
    }

    const number = Number.parseInt(rest.slice(0, digitsEnd), 10);
    if (!Number.isFinite(number) || number < 1 || number > expectedLines) {
      continue;
    }

    const afterDigits = rest.slice(digitsEnd);
    if (!afterDigits.startsWith("@@@")) {
      continue;
    }
    if (collected[number - 1] != null) {
      return null;
    }
    collected[number - 1] = afterDigits.slice(3);
  }

  if (collected.every((item) => item != null)) {
    return collected as string[];
  }
  return null;
}

export function collapseLineBreaksToSpaces(text: string) {
  if (!text.includes("\n") && !text.includes("\r")) {
    return text;
  }
  const normalized = normalizeLineEndingsToLf(text);
  let out = "";
  let lastWasSpace = false;

  for (const char of normalized) {
    if (char === "\n") {
      if (!lastWasSpace) {
        out += " ";
        lastWasSpace = true;
      }
      continue;
    }
    out += char;
    lastWasSpace = char === " ";
  }

  return out.trim();
}

export function finalizeMultilineSelectionCandidate(sourceText: string, candidateText: string) {
  const sourceLines = splitLinesKeepEmpty(sourceText);
  if (sourceLines.every((line) => !line.trim())) {
    return sourceText;
  }

  const expected = Math.max(sourceLines.length, 1);
  let rewrittenLines: string[];
  const parsed = tryParseMultilineRewriteResponse(candidateText, expected);
  if (parsed) {
    if (!blankPatternMatches(sourceLines, parsed)) {
      throw new Error("模型输出未保持原始空行结构。");
    }
    rewrittenLines = parsed;
  } else {
    const candidateLines = splitLinesKeepEmpty(candidateText);
    const numberingChanged = candidateLines.some((line) =>
      trimAsciiSpacesTabsStart(line).startsWith("@@@")
    );
    if (
      candidateLines.length !== expected ||
      !blankPatternMatches(sourceLines, candidateLines) ||
      numberingChanged
    ) {
      throw new Error("模型输出未按要求保持逐行结构。");
    }
    rewrittenLines = candidateLines;
  }

  const enforced: string[] = [];
  for (let index = 0; index < sourceLines.length; index += 1) {
    enforced.push(enforceLineSkeleton(sourceLines[index], rewrittenLines[index] ?? ""));
  }
  return enforced.join("\n");
}

export function finalizeSinglelineSelectionCandidate(sourceText: string, candidateText: string) {
  const { prefix, core, suffix } = splitLineSkeleton(sourceText);
  if (!core.trim()) {
    return sourceText;
  }
  const rewritten = collapseLineBreaksToSpaces(candidateText);
  const body = stripRedundantPrefix(rewritten, prefix);
  if (!body.trim()) {
    return sourceText;
  }
  return `${prefix}${body}${suffix}`;
}

export function finalizePlainSelectionCandidate(sourceText: string, candidateText: string) {
  const candidate =
    sourceText.includes("\n") || sourceText.includes("\r")
      ? finalizeMultilineSelectionCandidate(sourceText, candidateText)
      : finalizeSinglelineSelectionCandidate(sourceText, candidateText);
  validateSelectionRewriteOutput(sourceText, candidate);
  return candidate;
}

