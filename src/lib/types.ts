export type SegmentationPreset = "clause" | "sentence" | "paragraph";
export type RewriteMode = "manual" | "auto";
export type PromptPresetId = "aigc_v1" | "humanizer_zh" | (string & {});
export type RewriteUnitStatus = "idle" | "running" | "done" | "failed";
export type DiffType = "unchanged" | "insert" | "delete";
export type RunningState = "idle" | "running" | "paused" | "completed" | "cancelled" | "failed";
export type SuggestionDecision = "proposed" | "applied" | "dismissed";
export type WritebackSlotRole =
  | "editableText"
  | "lockedText"
  | "syntaxToken"
  | "inlineObject"
  | "paragraphBreak";

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
  segmentationPreset: SegmentationPreset;
  /** 是否允许改写标题/章节标题等结构性文本 */
  rewriteHeadings: boolean;
  rewriteMode: RewriteMode;
  maxConcurrency: number;
  unitsPerBatch: number;
  promptPresetId: PromptPresetId;
  customPrompts: PromptTemplate[];
}

export interface DiffSpan {
  type: DiffType;
  text: string;
}

export interface TextPresentation {
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

export interface WritebackSlot {
  id: string;
  order: number;
  text: string;
  editable: boolean;
  role: WritebackSlotRole;
  presentation: TextPresentation | null;
  anchor: string | null;
  separatorAfter: string;
}

export interface RewriteUnit {
  id: string;
  order: number;
  slotIds: string[];
  displayText: string;
  segmentationPreset: SegmentationPreset;
  status: RewriteUnitStatus;
  errorMessage: string | null;
}

export interface SlotUpdate {
  slotId: string;
  text: string;
}

export interface EditorSlotEdit {
  slotId: string;
  text: string;
}

export interface RewriteSuggestion {
  id: string;
  sequence: number;
  rewriteUnitId: string;
  beforeText: string;
  afterText: string;
  diffSpans: DiffSpan[];
  decision: SuggestionDecision;
  slotUpdates: SlotUpdate[];
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
  segmentationPreset?: SegmentationPreset | null;
  rewriteHeadings?: boolean | null;
  writebackSlots: WritebackSlot[];
  rewriteUnits: RewriteUnit[];
  suggestions: RewriteSuggestion[];
  nextSuggestionSequence: number;
  status: RunningState;
  createdAt: string;
  updatedAt: string;
}

export interface RewriteProgress {
  sessionId: string;
  completedUnits: number;
  inFlight: number;
  runningUnitIds: string[];
  totalUnits: number;
  mode: RewriteMode;
  runningState: RunningState;
  maxConcurrency: number;
}

export interface ProviderCheckResult {
  ok: boolean;
  message: string;
}
