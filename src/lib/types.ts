export type Theme = "system" | "light" | "dark";
export type BackendPreference = "auto" | "vulkan" | "cpu";
export type NoticeTone = "info" | "success" | "error";

export interface AppSettings {
  modelPath: string;
  mmprojPath: string;
  modelPreset: string;
  hotkey: string;
  theme: Theme;
  avatarSize: number;
  startWithWindows: boolean;
  systemPrompt: string;
  maxTokens: number;
  temperature: number;
  backend: BackendPreference;
  showPerformance: boolean;
  avatarX: number | null;
  avatarY: number | null;
}

export type ServerPhase =
  | "stopped"
  | "modelmissing"
  | "starting"
  | "warming"
  | "ready"
  | "error";

export interface ServerStatus {
  phase: ServerPhase;
  backend: string | null;
  message: string;
  port: number | null;
  modelName: string;
  restarts: number;
  startedAt: number | null;
  lastError: string | null;
}

export interface ModelFileInfo {
  presetId: string;
  modelPath: string;
  mmprojPath: string;
  modelPresent: boolean;
  mmprojPresent: boolean;
  modelBytes: number;
  mmprojBytes: number;
}

export interface Conversation {
  id: string;
  title: string;
  createdAt: number;
  updatedAt: number;
  messageCount: number;
}

export interface StoredMessage {
  id: string;
  conversationId: string;
  role: "user" | "assistant" | "system";
  content: string;
  imagePaths: string[];
  createdAt: number;
  ttftMs: number | null;
  tokensPerSecond: number | null;
  promptTokens: number | null;
  completionTokens: number | null;
  cacheTokens: number | null;
}

export interface Attachment {
  path: string;
  dataUrl: string;
  width: number;
  height: number;
}

export interface Bootstrap {
  settings: AppSettings;
  server: ServerStatus;
  model: ModelFileInfo;
  conversations: Conversation[];
  appVersion: string;
  dataDir: string;
  startedAt: number;
}

export interface SendMessageRequest {
  conversationId: string | null;
  content: string;
  imagePaths: string[];
}

export interface SendMessageResult {
  conversationId: string;
  userMessage: StoredMessage;
  requestId: string;
}

export interface ChatDelta {
  requestId: string;
  conversationId: string;
  delta: string;
}

export interface ChatDone {
  requestId: string;
  conversationId: string;
  message: StoredMessage;
  contextTrimmed: boolean;
}

export interface ChatFailed {
  requestId: string;
  conversationId: string;
  message: string;
  partial: string;
  retryable: boolean;
}

export interface SnipReady {
  attachment: Attachment;
  source: string;
  panelWasVisible: boolean;
}

export interface OverlayPayload {
  label: string;
  name: string;
  monitorX: number;
  monitorY: number;
  width: number;
  height: number;
  scaleFactor: number;
  viewportWidth: number;
  viewportHeight: number;
  imageDataUrl: string;
}

export interface SnipSelection {
  label: string;
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface PerformanceSample {
  id: number;
  kind: string;
  valueMs: number;
  detail: string | null;
  createdAt: number;
}

export interface Diagnostics {
  server: ServerStatus;
  model: ModelFileInfo;
  appVersion: string;
  llamaBuild: string;
  os: string;
  cpu: string;
  memoryTotalBytes: number;
  processMemoryBytes: number;
  processCpuPercent: number;
  recentLogs: string[];
  performance: PerformanceSample[];
  lastTokensPerSecond: number | null;
  dataDir: string;
}

export interface DownloadProgress {
  presetId: string;
  file: string;
  downloaded: number;
  total: number;
  percent: number;
  speedBytesPerSecond: number;
  error: string | null;
  done: boolean;
}

export interface ServerMetrics {
  ttftMs: number | null;
  tokensPerSecond: number | null;
  promptTokens: number | null;
  completionTokens: number | null;
  cacheTokens: number | null;
  totalMs: number;
  conversationId: string;
}

export type AvatarState =
  | "idle"
  | "warming"
  | "listening"
  | "thinking"
  | "answering"
  | "error";

export interface PresetInfo {
  id: string;
  label: string;
  note: string;
  modelFile: string;
  mmprojFile: string;
  modelBytes: number;
  mmprojBytes: number;
}

export const PRESETS: PresetInfo[] = [
  {
    id: "e2b",
    label: "Faster",
    note: "Gemma 4 E2B · Q4_0 · quickest replies, lighter memory",
    modelFile: "gemma-4-E2B-it-Q4_0.gguf",
    mmprojFile: "mmproj-gemma-4-E2B-it-Q8_0.gguf",
    modelBytes: 2_841_481_184,
    mmprojBytes: 557_368_064,
  },
  {
    id: "e4b",
    label: "Balanced",
    note: "Gemma 4 E4B · Q4_0 · recommended for most machines",
    modelFile: "gemma-4-E4B-it-Q4_0.gguf",
    mmprojFile: "mmproj-gemma-4-E4B-it-Q8_0.gguf",
    modelBytes: 4_590_807_392,
    mmprojBytes: 559_874_816,
  },
  {
    id: "12b",
    label: "Smarter",
    note: "Gemma 4 12B · Q4_0 · slower, needs about 9 GB free RAM",
    modelFile: "gemma-4-12B-it-Q4_0.gguf",
    mmprojFile: "mmproj-gemma-4-12B-it-Q8_0.gguf",
    modelBytes: 7_219_673_216,
    mmprojBytes: 158_987_616,
  },
];

export function presetById(id: string): PresetInfo {
  return PRESETS.find((preset) => preset.id === id) ?? PRESETS[1]!;
}
