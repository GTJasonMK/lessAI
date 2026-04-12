export type ChunkPreset = "clause" | "sentence" | "paragraph";
export type RewriteMode = "manual" | "auto";
export type PromptPresetId = "aigc_v1" | "humanizer_zh" | (string & {});
export type ChunkStatus = "idle" | "running" | "done" | "failed";
export type DiffType = "unchanged" | "insert" | "delete";
export type RunningState = "idle" | "running" | "paused" | "completed" | "cancelled" | "failed";
export type SuggestionDecision = "proposed" | "applied" | "dismissed";

export interface PromptTemplate {
  id: string;
  name: string;
  content: string;
}

export interface AppSettings {
  baseUrl: string;
  apiKey: string;
  model: string;
  /**
   * 检查更新/下载更新使用的代理（可选）。
   * 为空字符串表示直连。
   */
  updateProxy: string;
  timeoutMs: number;
  temperature: number;
  chunkPreset: ChunkPreset;
  /** 是否允许改写标题/章节标题等结构性文本 */
  rewriteHeadings: boolean;
  rewriteMode: RewriteMode;
  maxConcurrency: number;
  promptPresetId: PromptPresetId;
  customPrompts: PromptTemplate[];
}

export interface DiffSpan {
  type: DiffType;
  text: string;
}

export interface ChunkPresentation {
  bold: boolean;
  italic: boolean;
  underline: boolean;
  href: string | null;
  protectKind: string | null;
  writebackKey?: string | null;
}

export interface DocumentSnapshot {
  sha256: string;
}

export interface ChunkTask {
  index: number;
  sourceText: string;
  /** 片段后的拼接分隔符，用于导出时还原段落/句子边界 */
  separatorAfter: string;
  /** 是否跳过 AI 改写（例如 Markdown fenced code block） */
  skipRewrite: boolean;
  presentation: ChunkPresentation | null;
  status: ChunkStatus;
  errorMessage: string | null;
}

export interface EditorChunkEdit {
  index: number;
  text: string;
}

export interface EditSuggestion {
  id: string;
  sequence: number;
  chunkIndex: number;
  beforeText: string;
  afterText: string;
  diffSpans: DiffSpan[];
  decision: SuggestionDecision;
  createdAt: string;
  updatedAt: string;
}

export interface DocumentSession {
  id: string;
  title: string;
  documentPath: string;
  sourceText: string;
  sourceSnapshot?: DocumentSnapshot | null;
  normalizedText: string;
  writeBackSupported: boolean;
  writeBackBlockReason: string | null;
  plainTextEditorSafe: boolean;
  plainTextEditorBlockReason: string | null;
  chunkPreset?: ChunkPreset | null;
  rewriteHeadings?: boolean | null;
  chunks: ChunkTask[];
  suggestions: EditSuggestion[];
  nextSuggestionSequence: number;
  status: RunningState;
  createdAt: string;
  updatedAt: string;
}

export interface RewriteProgress {
  sessionId: string;
  completedChunks: number;
  inFlight: number;
  runningIndices: number[];
  totalChunks: number;
  mode: RewriteMode;
  runningState: RunningState;
  maxConcurrency: number;
}

export interface ProviderCheckResult {
  ok: boolean;
  message: string;
}
