import type {
  AppSettings,
  SegmentationPreset,
  RewriteMode,
} from "./types";

export type NoticeTone = "info" | "success" | "warning" | "error";

export interface NoticeState {
  tone: NoticeTone;
  message: string;
  autoDismissMs?: number | null;
}

export interface RewriteUnitCompletedPayload {
  sessionId: string;
  rewriteUnitId: string;
  suggestionId: string;
  suggestionSequence: number;
}

export interface SessionEventPayload {
  sessionId: string;
}

export interface RewriteFailedPayload {
  sessionId: string;
  error: string;
}

export interface PanelProps {
  title: string;
  subtitle?: string;
  action?: React.ReactNode;
  footer?: React.ReactNode;
  className?: string;
  bodyClassName?: string;
  children: React.ReactNode;
}

export const TAURI_EVENTS = {
  REWRITE_PROGRESS: "rewrite_progress",
  REWRITE_UNIT_COMPLETED: "rewrite_unit_completed",
  REWRITE_FINISHED: "rewrite_finished",
  REWRITE_FAILED: "rewrite_failed"
} as const;

export const DEFAULT_SETTINGS: AppSettings = {
  baseUrl: "https://api.openai.com/v1",
  apiKey: "",
  model: "gpt-4.1-mini",
  updateProxy: "",
  timeoutMs: 45_000,
  temperature: 0.8,
  segmentationPreset: "paragraph",
  rewriteHeadings: false,
  rewriteMode: "manual",
  maxConcurrency: 2,
  unitsPerBatch: 1,
  promptPresetId: "humanizer_zh",
  customPrompts: []
};

export const PRESET_OPTIONS: ReadonlyArray<{
  value: SegmentationPreset;
  label: string;
  hint: string;
}> = [
  { value: "clause", label: "小句", hint: "按逗号/分号等切分，粒度最细（更稳妥但调用更多）" },
  { value: "sentence", label: "整句", hint: "按句号/问号等切分，粒度折中（更适合长段落）" },
  {
    value: "paragraph",
    label: "空白分段",
    hint: "按空白分段（TeX：空行/\\par 分段；单换行视为段内空格）"
  }
];

export const MODE_OPTIONS: ReadonlyArray<{
  value: RewriteMode;
  label: string;
  hint: string;
}> = [
  { value: "manual", label: "人工把关", hint: "逐段生成，等待你审核" },
  { value: "auto", label: "自动批处理", hint: "后台连续生成，可按并发数提速" }
];
