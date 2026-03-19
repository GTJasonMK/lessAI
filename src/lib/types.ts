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
  timeoutMs: number;
  temperature: number;
  chunkPreset: ChunkPreset;
  rewriteMode: RewriteMode;
  maxConcurrency: number;
  promptPresetId: PromptPresetId;
  customPrompts: PromptTemplate[];
}

export interface DiffSpan {
  type: DiffType;
  text: string;
}

export interface ChunkTask {
  index: number;
  sourceText: string;
  /** 片段后的拼接分隔符，用于导出时还原段落/句子边界 */
  separatorAfter: string;
  status: ChunkStatus;
  errorMessage: string | null;
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
  normalizedText: string;
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
