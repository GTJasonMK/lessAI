import type {
  AppSettings,
  ChunkPreset,
  RewriteMode
} from "./types";

// ── UI 类型 ──────────────────────────────────────────────

export type NoticeTone = "info" | "success" | "warning" | "error";
export type ReviewView = "diff" | "source" | "candidate";

// ── 接口 ─────────────────────────────────────────────────

export interface NoticeState {
  tone: NoticeTone;
  message: string;
}

export interface ChunkCompletedPayload {
  sessionId: string;
  index: number;
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

// ── Tauri 事件名常量 ────────────────────────────────────

export const TAURI_EVENTS = {
  REWRITE_PROGRESS: "rewrite_progress",
  CHUNK_COMPLETED: "chunk_completed",
  REWRITE_FINISHED: "rewrite_finished",
  REWRITE_FAILED: "rewrite_failed"
} as const;

// ── 默认值 ──────────────────────────────────────────────

export const DEFAULT_SETTINGS: AppSettings = {
  baseUrl: "https://api.openai.com/v1",
  apiKey: "",
  model: "gpt-4.1-mini",
  timeoutMs: 45_000,
  temperature: 0.8,
  chunkPreset: "sentence",
  rewriteMode: "manual",
  maxConcurrency: 2,
  promptPresetId: "humanizer_zh",
  customPrompts: []
};

// ── 选项配置 ─────────────────────────────────────────────

export const PRESET_OPTIONS: ReadonlyArray<{
  value: ChunkPreset;
  label: string;
  hint: string;
}> = [
  { value: "clause", label: "小句", hint: "按逗号/分号等切分，最细粒度" },
  { value: "sentence", label: "整句", hint: "按句号/问号等切分，默认推荐" },
  { value: "paragraph", label: "段落", hint: "按自然段切分，轮次更少" }
];

export const MODE_OPTIONS: ReadonlyArray<{
  value: RewriteMode;
  label: string;
  hint: string;
}> = [
  { value: "manual", label: "人工把关", hint: "逐段生成，等待你审核" },
  { value: "auto", label: "自动批处理", hint: "后台连续生成，可按并发数提速" }
];

export const REVIEW_VIEW_OPTIONS: ReadonlyArray<{
  key: ReviewView;
  label: string;
}> = [
  { key: "diff", label: "Diff" },
  { key: "source", label: "原文" },
  { key: "candidate", label: "候选稿" }
];
