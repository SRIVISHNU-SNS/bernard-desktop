import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type ReactNode,
} from "react";
import { api } from "../lib/bridge";
import { applyTheme } from "../lib/theme";
import {
  formatBytes,
  formatDuration,
  formatRelative,
  formatSpeed,
  messageOf,
  percent,
} from "../lib/format";
import { fileName, joinModelsPath } from "../lib/files";
import { hotkeyFromEvent, prettyHotkey } from "../lib/hotkey";
import {
  PRESETS,
  presetById,
  type AppSettings,
  type Diagnostics,
  type DownloadProgress,
  type ModelFileInfo,
  type NoticeTone,
  type ServerStatus,
} from "../lib/types";
import {
  AlertIcon,
  CheckIcon,
  ChevronRightIcon,
  CloseIcon,
  CopyIcon,
  CpuIcon,
  DownloadIcon,
  FolderIcon,
  LogoMark,
  MemoryIcon,
  MonitorIcon,
  RefreshIcon,
  ShieldIcon,
  SparkIcon,
  SunIcon,
  TrashIcon,
} from "../lib/icons";

interface SettingsViewProps {
  settings: AppSettings;
  model: ModelFileInfo | null;
  server: ServerStatus | null;
  dataDir: string;
  appVersion: string;
  download: DownloadProgress | null;
  downloadingPreset: string | null;
  onBack: () => void;
  onSettings: (settings: AppSettings) => void;
  onModel: (model: ModelFileInfo) => void;
  onDownload: (presetId: string) => Promise<void>;
  onCancelDownload: () => void;
  onNotice: (message: string, tone?: NoticeTone) => void;
  onDataCleared: () => void;
}

function normalisePath(path: string): string {
  return path.replace(/\//g, "\\").toLowerCase();
}

function phaseCopy(status: ServerStatus | null): {
  label: string;
  tone: "ready" | "busy" | "error";
} {
  if (!status) return { label: "Starting", tone: "busy" };
  switch (status.phase) {
    case "ready":
      return { label: "Ready", tone: "ready" };
    case "warming":
      return { label: "Warming up", tone: "busy" };
    case "starting":
      return { label: "Starting", tone: "busy" };
    case "modelmissing":
      return { label: "Model missing", tone: "error" };
    case "error":
      return { label: "Needs attention", tone: "error" };
    default:
      return { label: "Stopped", tone: "busy" };
  }
}

function performanceValue(kind: string, value: number): string {
  if (kind === "tokens_per_second") return `${value.toFixed(1)} tok/s`;
  return formatDuration(value);
}

function Section({
  icon,
  title,
  description,
  children,
  danger,
}: {
  icon: ReactNode;
  title: string;
  description: string;
  children: ReactNode;
  danger?: boolean;
}) {
  return (
    <section className={`settings-card${danger ? " settings-card--danger" : ""}`}>
      <header className="settings-card-head">
        <span className="settings-card-icon" aria-hidden>
          {icon}
        </span>
        <div>
          <h2>{title}</h2>
          <p>{description}</p>
        </div>
      </header>
      <div className="settings-card-body">{children}</div>
    </section>
  );
}

function Switch({
  checked,
  onChange,
  label,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
}) {
  return (
    <button
      type="button"
      className={`switch${checked ? " switch--on" : ""}`}
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
    >
      <span className="switch-thumb" aria-hidden />
    </button>
  );
}

export function SettingsView({
  settings,
  model,
  server,
  dataDir,
  appVersion,
  download,
  downloadingPreset,
  onBack,
  onSettings,
  onModel,
  onDownload,
  onCancelDownload,
  onNotice,
  onDataCleared,
}: SettingsViewProps) {
  const [draft, setDraft] = useState<AppSettings>(settings);
  const [dirty, setDirty] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [recording, setRecording] = useState(false);
  const [hotkeyHint, setHotkeyHint] = useState<string | null>(null);
  const [picking, setPicking] = useState<"model" | "mmproj" | null>(null);
  const [diag, setDiag] = useState<Diagnostics | null>(null);
  const [diagError, setDiagError] = useState<string | null>(null);
  const [diagLoading, setDiagLoading] = useState(false);
  const [confirmClear, setConfirmClear] = useState(false);
  const [confirmReset, setConfirmReset] = useState(false);
  const [clearing, setClearing] = useState(false);
  const [resetting, setResetting] = useState(false);
  const [confirmDiscard, setConfirmDiscard] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    setDraft(settings);
    setDirty(false);
    setSaveError(null);
  }, [settings]);

  useEffect(() => {
    applyTheme(draft.theme);
  }, [draft.theme]);

  const loadDiagnostics = useCallback(async () => {
    setDiagLoading(true);
    setDiagError(null);
    try {
      setDiag(await api.diagnostics());
    } catch (error) {
      setDiagError(messageOf(error));
    } finally {
      setDiagLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadDiagnostics();
  }, [loadDiagnostics]);

  const patch = (next: Partial<AppSettings>) => {
    setDraft((current) => ({ ...current, ...next }));
    setDirty(true);
    setSaveError(null);
  };

  const save = async (): Promise<boolean> => {
    if (!draft.systemPrompt.trim()) {
      setSaveError("The system prompt cannot be empty.");
      return false;
    }
    setSaving(true);
    setSaveError(null);
    try {
      const saved = await api.saveSettings(draft);
      setDraft(saved);
      setDirty(false);
      onSettings(saved);
      try {
        onModel(await api.modelInfo());
      } catch {
        // The live server status will explain any model problem.
      }
      onNotice("Settings saved.", "success");
      void loadDiagnostics();
      return true;
    } catch (error) {
      setSaveError(messageOf(error));
      return false;
    } finally {
      setSaving(false);
    }
  };

  const requestBack = () => {
    if (dirty) {
      setConfirmDiscard(true);
      return;
    }
    applyTheme(settings.theme);
    onBack();
  };

  const discard = () => {
    setConfirmDiscard(false);
    applyTheme(settings.theme);
    onBack();
  };

  const choosePreset = (id: string) => {
    const preset = presetById(id);
    patch({
      modelPreset: id,
      modelPath: joinModelsPath(dataDir, preset.modelFile),
      mmprojPath: joinModelsPath(dataDir, preset.mmprojFile),
    });
  };

  const pickFile = async (kind: "model" | "mmproj") => {
    setPicking(kind);
    try {
      const picked = await api.pickModelFile(kind);
      if (picked) {
        patch(kind === "model" ? { modelPath: picked } : { mmprojPath: picked });
      }
    } catch (error) {
      onNotice(messageOf(error), "error");
    } finally {
      setPicking(null);
    }
  };

  const preset = presetById(draft.modelPreset);
  const managedModel = joinModelsPath(dataDir, preset.modelFile);
  const managedMmproj = joinModelsPath(dataDir, preset.mmprojFile);
  const customPaths =
    normalisePath(draft.modelPath) !== normalisePath(managedModel) ||
    normalisePath(draft.mmprojPath) !== normalisePath(managedMmproj);
  const savedMatchesDraft =
    settings.modelPreset === draft.modelPreset &&
    normalisePath(settings.modelPath) === normalisePath(draft.modelPath) &&
    normalisePath(settings.mmprojPath) === normalisePath(draft.mmprojPath);
  const installed = savedMatchesDraft && Boolean(model?.modelPresent && model?.mmprojPresent);
  const currentDownload = download && download.presetId === draft.modelPreset ? download : null;
  const downloadingSomething = downloadingPreset !== null;
  const status = phaseCopy(server);

  const onHotkeyKey = (event: ReactKeyboardEvent<HTMLButtonElement>) => {
    if (!recording) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.key === "Escape") {
      setRecording(false);
      setHotkeyHint(null);
      return;
    }
    const combo = hotkeyFromEvent(event);
    if (combo) {
      patch({ hotkey: combo });
      setRecording(false);
      setHotkeyHint(null);
    } else {
      setHotkeyHint("Use at least one modifier, such as Alt+B. Function keys work alone.");
    }
  };

  const clearAll = async () => {
    setClearing(true);
    try {
      await api.clearAllData();
      setConfirmClear(false);
      onDataCleared();
      onNotice("Every conversation and snip was removed.", "success");
      void loadDiagnostics();
    } catch (error) {
      onNotice(messageOf(error), "error");
    } finally {
      setClearing(false);
    }
  };

  const resetEverything = async () => {
    setResetting(true);
    try {
      const defaults = await api.resetSettings();
      setDraft(defaults);
      setDirty(false);
      setConfirmReset(false);
      onSettings(defaults);
      onNotice("Settings restored to their defaults.", "success");
      try {
        onModel(await api.modelInfo());
      } catch {
        // The server status will catch up on its own.
      }
      void loadDiagnostics();
    } catch (error) {
      onNotice(messageOf(error), "error");
    } finally {
      setResetting(false);
    }
  };

  const copyReport = async () => {
    if (!diag) return;
    const report = [
      `Bernard ${diag.appVersion} · llama.cpp ${diag.llamaBuild}`,
      `Server: ${diag.server.phase}${diag.server.backend ? ` (${diag.server.backend})` : ""} · port ${diag.server.port ?? "—"} · restarts ${diag.server.restarts}`,
      `Model: ${diag.model.presetId} · model ${diag.model.modelPresent ? "present" : "missing"} · mmproj ${diag.model.mmprojPresent ? "present" : "missing"}`,
      `OS: ${diag.os}`,
      `CPU: ${diag.cpu}`,
      `Memory: ${formatBytes(diag.processMemoryBytes)} process · ${formatBytes(diag.memoryTotalBytes)} total`,
      `CPU load: ${diag.processCpuPercent.toFixed(1)}%`,
      `Last tok/s: ${diag.lastTokensPerSecond?.toFixed(1) ?? "—"}`,
      `Data: ${diag.dataDir}`,
      "",
      "Recent performance",
      ...diag.performance
        .slice(0, 12)
        .map(
          (sample) =>
            `${sample.kind}: ${performanceValue(sample.kind, sample.valueMs)}${sample.detail ? ` (${sample.detail})` : ""}`,
        ),
      "",
      "Logs",
      ...diag.recentLogs,
    ].join("\n");
    try {
      await api.copyText(report);
      onNotice("Diagnostic report copied.", "success");
    } catch (error) {
      onNotice(messageOf(error), "error");
    }
  };

  const modelsDir = useMemo(() => joinModelsPath(dataDir, "").replace(/\\$/, ""), [dataDir]);

  return (
    <div className="settings-shell" ref={rootRef}>
      <header className="settings-head">
        <button
          type="button"
          className="icon-button settings-back"
          onClick={requestBack}
          aria-label="Back to chat"
          title="Back to chat"
        >
          <ChevronRightIcon size={18} />
        </button>
        <div className="settings-heading">
          <h1>Settings</h1>
          <span>
            {status.label}
            {server?.backend ? ` · ${server.backend}` : ""} · on this PC
          </span>
        </div>
        <button
          type="button"
          className="icon-button"
          onClick={requestBack}
          aria-label="Close settings"
          title="Close settings"
        >
          <CloseIcon size={16} />
        </button>
      </header>

      <div className="settings-scroll">
        <Section
          icon={<CpuIcon size={17} />}
          title="The model"
          description="Choose how much of Gemma 4 to run. Everything stays on this machine."
        >
          <div className="preset-grid" role="radiogroup" aria-label="Model size">
            {PRESETS.map((choice) => {
              const selected = choice.id === draft.modelPreset;
              const isInstalled =
                savedMatchesDraft && choice.id === settings.modelPreset && installed;
              return (
                <button
                  key={choice.id}
                  type="button"
                  className={`preset-card${selected ? " preset-card--selected" : ""}`}
                  role="radio"
                  aria-checked={selected}
                  onClick={() => choosePreset(choice.id)}
                >
                  <span className="preset-top">
                    <span className="preset-radio" aria-hidden />
                    <span className="preset-label">{choice.label}</span>
                    {isInstalled ? (
                      <span className="preset-tag">
                        <CheckIcon size={10} />
                        Ready
                      </span>
                    ) : null}
                  </span>
                  <span className="preset-note">{choice.note}</span>
                  <span className="preset-size">
                    {formatBytes(choice.modelBytes + choice.mmprojBytes)} download
                  </span>
                </button>
              );
            })}
          </div>

          {draft.modelPreset === "12b" ? (
            <p className="inline-warning">
              <AlertIcon size={15} />
              The Smarter model wants about 9 GB of free memory. If Bernard feels sluggish, switch
              back to Balanced.
            </p>
          ) : null}

          <div className="settings-field">
            <div className="field-head">
              <span>Model file</span>
              <span className="field-tag">{customPaths ? "Custom path" : "Managed"}</span>
            </div>
            <div className="path-row">
              <code title={draft.modelPath}>{draft.modelPath || "Not set"}</code>
              <button
                type="button"
                className="text-button"
                onClick={() => void pickFile("model")}
                disabled={picking !== null}
              >
                {picking === "model" ? "Choosing…" : "Choose…"}
              </button>
            </div>
          </div>

          <div className="settings-field">
            <div className="field-head">
              <span>Vision projector (mmproj)</span>
              {customPaths ? (
                <button
                  type="button"
                  className="field-link"
                  onClick={() =>
                    patch({ modelPath: managedModel, mmprojPath: managedMmproj })
                  }
                >
                  Use managed paths
                </button>
              ) : (
                <span className="field-tag">Managed</span>
              )}
            </div>
            <div className="path-row">
              <code title={draft.mmprojPath}>{draft.mmprojPath || "Not set"}</code>
              <button
                type="button"
                className="text-button"
                onClick={() => void pickFile("mmproj")}
                disabled={picking !== null}
              >
                {picking === "mmproj" ? "Choosing…" : "Choose…"}
              </button>
            </div>
          </div>

          <div className="download-zone">
            {currentDownload ? (
              <div className="download-card" aria-live="polite">
                <div className="download-head">
                  <span>{currentDownload.file || fileName(preset.modelFile)}</span>
                  <strong>{percent(currentDownload.downloaded, currentDownload.total).toFixed(0)}%</strong>
                </div>
                <div className="progress-track">
                  <span
                    style={{
                      width: `${percent(currentDownload.downloaded, currentDownload.total)}%`,
                    }}
                  />
                </div>
                <div className="download-meta">
                  {currentDownload.total > 0
                    ? `${formatBytes(currentDownload.downloaded)} of ${formatBytes(currentDownload.total)}`
                    : "Contacting the model host…"}
                  <span aria-hidden> · </span>
                  {formatSpeed(currentDownload.speedBytesPerSecond)}
                </div>
              </div>
            ) : null}

            <div className="download-actions">
              {installed ? (
                <span className="ready-pill">
                  <CheckIcon size={13} />
                  Model files are ready
                </span>
              ) : (
                <button
                  type="button"
                  className="primary-button"
                  onClick={() => void onDownload(draft.modelPreset)}
                  disabled={downloadingSomething}
                >
                  <DownloadIcon size={15} />
                  {downloadingSomething
                    ? `Downloading ${presetById(downloadingPreset ?? draft.modelPreset).label}…`
                    : `Download ${preset.label} model`}
                </button>
              )}
              {downloadingSomething ? (
                <button type="button" className="text-button" onClick={onCancelDownload}>
                  Cancel download
                </button>
              ) : null}
            </div>
            <p className="field-hint">
              Downloads about {formatBytes(preset.modelBytes + preset.mmprojBytes)} to{" "}
              <code>{modelsDir}</code>. Existing verified files are reused.
            </p>
          </div>

          <div className="settings-field">
            <div className="field-head">
              <span>Compute backend</span>
            </div>
            <div className="segmented" role="radiogroup" aria-label="Compute backend">
              {(
                [
                  ["auto", "Auto"],
                  ["vulkan", "Vulkan"],
                  ["cpu", "CPU"],
                ] as const
              ).map(([value, label]) => (
                <button
                  key={value}
                  type="button"
                  role="radio"
                  aria-checked={draft.backend === value}
                  className={draft.backend === value ? "is-selected" : ""}
                  onClick={() => patch({ backend: value })}
                >
                  {label}
                </button>
              ))}
            </div>
            <p className="field-hint">
              Auto prefers your GPU through Vulkan and falls back to the CPU if it cannot start.
            </p>
          </div>
        </Section>

        <Section
          icon={<SparkIcon size={17} />}
          title="How Bernard answers"
          description="Tune the local model's voice and the length of its replies."
        >
          <label className="settings-field">
            <span className="field-head">
              <span>System prompt</span>
              <span className="field-tag">{draft.systemPrompt.length} chars</span>
            </span>
            <textarea
              className="settings-textarea"
              value={draft.systemPrompt}
              onChange={(event) => patch({ systemPrompt: event.target.value })}
              rows={6}
              spellCheck
            />
            <span className="field-hint">
              This is sent before every conversation. Keep it specific about tone and honesty.
            </span>
          </label>

          <div className="settings-field">
            <div className="field-head">
              <span>Longest reply</span>
              <span className="field-value">{draft.maxTokens} tokens</span>
            </div>
            <input
              type="range"
              className="slider"
              min={64}
              max={4096}
              step={32}
              value={draft.maxTokens}
              onChange={(event) => patch({ maxTokens: Number(event.target.value) })}
              aria-label="Maximum reply tokens"
            />
            <p className="field-hint">
              A cap, not a target. Longer limits leave less room for conversation history.
            </p>
          </div>

          <div className="settings-field">
            <div className="field-head">
              <span>Temperature</span>
              <span className="field-value">{draft.temperature.toFixed(2)}</span>
            </div>
            <input
              type="range"
              className="slider"
              min={0}
              max={2}
              step={0.05}
              value={draft.temperature}
              onChange={(event) => patch({ temperature: Number(event.target.value) })}
              aria-label="Temperature"
            />
            <p className="field-hint">Lower is steadier; higher is more playful.</p>
          </div>

          <div className="settings-toggle-row">
            <div>
              <span className="toggle-title">Show performance under replies</span>
              <span className="toggle-sub">First token time, tokens per second, and prompt counts.</span>
            </div>
            <Switch
              checked={draft.showPerformance}
              onChange={(checked) => patch({ showPerformance: checked })}
              label="Show performance under replies"
            />
          </div>
        </Section>

        <Section
          icon={<SunIcon size={17} />}
          title="Appearance"
          description="The avatar, the panel, and how Bernard meets your desktop."
        >
          <div className="settings-field">
            <div className="field-head">
              <span>Theme</span>
            </div>
            <div className="segmented" role="radiogroup" aria-label="Theme">
              {(
                [
                  ["system", "System"],
                  ["light", "Light"],
                  ["dark", "Dark"],
                ] as const
              ).map(([value, label]) => (
                <button
                  key={value}
                  type="button"
                  role="radio"
                  aria-checked={draft.theme === value}
                  className={draft.theme === value ? "is-selected" : ""}
                  onClick={() => patch({ theme: value })}
                >
                  {label}
                </button>
              ))}
            </div>
          </div>

          <div className="settings-field">
            <div className="field-head">
              <span>Avatar size</span>
              <span className="field-value">{draft.avatarSize} px</span>
            </div>
            <input
              type="range"
              className="slider"
              min={56}
              max={96}
              step={1}
              value={draft.avatarSize}
              onChange={(event) => patch({ avatarSize: Number(event.target.value) })}
              aria-label="Avatar size"
            />
            <p className="field-hint">
              The panel follows the avatar, so a larger face still leaves room on screen.
            </p>
          </div>
        </Section>

        <Section
          icon={<MonitorIcon size={17} />}
          title="Snips and startup"
          description="Bernard stays out of the way until you call."
        >
          <div className="settings-field">
            <div className="field-head">
              <span>Snip hotkey</span>
              <span className="field-tag">{recording ? "Listening" : "Global"}</span>
            </div>
            <button
              type="button"
              className={`hotkey-field${recording ? " is-recording" : ""}`}
              onClick={() => {
                setRecording(true);
                setHotkeyHint(null);
              }}
              onKeyDown={onHotkeyKey}
              onBlur={() => {
                setRecording(false);
                setHotkeyHint(null);
              }}
              aria-label="Change snip hotkey"
            >
              {recording ? "Press a key combination…" : prettyHotkey(draft.hotkey)}
            </button>
            <p className={`field-hint${hotkeyHint ? " field-hint--warn" : ""}`}>
              {hotkeyHint ??
                "Click and press a new combination. Windows reserves some shortcuts."}
            </p>
          </div>

          <div className="settings-toggle-row">
            <div>
              <span className="toggle-title">Start with Windows</span>
              <span className="toggle-sub">Bernard wakes quietly with the avatar already on screen.</span>
            </div>
            <Switch
              checked={draft.startWithWindows}
              onChange={(checked) => patch({ startWithWindows: checked })}
              label="Start with Windows"
            />
          </div>
        </Section>

        <Section
          icon={<ShieldIcon size={17} />}
          title="Privacy and data"
          description="No accounts, no telemetry, no screenshots leaving this PC."
        >
          <div className="data-row">
            <div>
              <span className="toggle-title">Everything stays here</span>
              <span className="toggle-sub">
                Conversations live in SQLite and snips live as PNG files under your app data folder.
              </span>
            </div>
            <button type="button" className="text-button" onClick={() => void api.openDataFolder()}>
              <FolderIcon size={14} />
              Open folder
            </button>
          </div>
          <p className="data-path" title={dataDir}>
            {dataDir || "App data folder"}
          </p>

          <div className="danger-zone">
            <div className="danger-row">
              <div>
                <span className="toggle-title">Clear all conversations and snips</span>
                <span className="toggle-sub">This cannot be undone. Settings and models stay.</span>
              </div>
              {confirmClear ? (
                <div className="confirm-inline">
                  <button
                    type="button"
                    className="danger-button"
                    onClick={() => void clearAll()}
                    disabled={clearing}
                  >
                    {clearing ? "Clearing…" : "Delete everything"}
                  </button>
                  <button type="button" className="text-button" onClick={() => setConfirmClear(false)}>
                    Cancel
                  </button>
                </div>
              ) : (
                <button type="button" className="text-button text-button--danger" onClick={() => setConfirmClear(true)}>
                  <TrashIcon size={14} />
                  Clear data
                </button>
              )}
            </div>

            <div className="danger-row">
              <div>
                <span className="toggle-title">Reset settings</span>
                <span className="toggle-sub">Return to the factory model, prompt, and appearance.</span>
              </div>
              {confirmReset ? (
                <div className="confirm-inline">
                  <button
                    type="button"
                    className="danger-button"
                    onClick={() => void resetEverything()}
                    disabled={resetting}
                  >
                    {resetting ? "Resetting…" : "Reset everything"}
                  </button>
                  <button type="button" className="text-button" onClick={() => setConfirmReset(false)}>
                    Cancel
                  </button>
                </div>
              ) : (
                <button type="button" className="text-button" onClick={() => setConfirmReset(true)}>
                  <RefreshIcon size={14} />
                  Reset
                </button>
              )}
            </div>
          </div>
        </Section>

        <Section
          icon={<AlertIcon size={17} />}
          title="Diagnostics"
          description="What the local server is doing, and how fast it is doing it."
        >
          <div className="diag-status">
            <span className={`diag-dot diag-dot--${status.tone}`} aria-hidden />
            <div>
              <span className="diag-status-title">
                {status.label}
                {server?.backend ? ` · ${server.backend}` : ""}
              </span>
              <span className="diag-status-sub">
                {server?.message || "The local server has not reported yet."}
              </span>
              {server?.lastError ? <code className="diag-error">{server.lastError}</code> : null}
            </div>
            <button
              type="button"
              className="text-button"
              onClick={() => {
                void api.restartServer();
                onNotice("Restarting the local model…", "info");
                window.setTimeout(() => void loadDiagnostics(), 1200);
              }}
            >
              <RefreshIcon size={14} />
              Restart
            </button>
          </div>

          {diagError ? (
            <div className="diag-empty">
              <span>{diagError}</span>
              <button type="button" className="text-button" onClick={() => void loadDiagnostics()}>
                Try again
              </button>
            </div>
          ) : diagLoading && !diag ? (
            <div className="diag-empty">
              <span className="button-spinner" aria-hidden />
              Reading the room…
            </div>
          ) : diag ? (
            <>
              <div className="diag-grid">
                <div className="diag-item">
                  <CpuIcon size={14} />
                  <span>CPU</span>
                  <strong>{diag.cpu}</strong>
                </div>
                <div className="diag-item">
                  <MemoryIcon size={14} />
                  <span>Memory</span>
                  <strong>
                    {formatBytes(diag.processMemoryBytes)} / {formatBytes(diag.memoryTotalBytes)}
                  </strong>
                </div>
                <div className="diag-item">
                  <SparkIcon size={14} />
                  <span>Process CPU</span>
                  <strong>{diag.processCpuPercent.toFixed(1)}%</strong>
                </div>
                <div className="diag-item">
                  <RefreshIcon size={14} />
                  <span>Last speed</span>
                  <strong>
                    {diag.lastTokensPerSecond ? `${diag.lastTokensPerSecond.toFixed(1)} tok/s` : "—"}
                  </strong>
                </div>
                <div className="diag-item">
                  <MonitorIcon size={14} />
                  <span>System</span>
                  <strong>{diag.os}</strong>
                </div>
                <div className="diag-item">
                  <DownloadIcon size={14} />
                  <span>Llama.cpp</span>
                  <strong>{diag.llamaBuild}</strong>
                </div>
              </div>

              {diag.performance.length > 0 ? (
                <div className="diag-table">
                  <div className="diag-table-head">
                    <span>Recent measurements</span>
                    <span>Value</span>
                    <span>When</span>
                  </div>
                  {diag.performance.slice(0, 10).map((sample) => (
                    <div className="diag-table-row" key={sample.id} title={sample.detail ?? undefined}>
                      <span>{sample.kind.replaceAll("_", " ")}</span>
                      <strong>{performanceValue(sample.kind, sample.valueMs)}</strong>
                      <span>{formatRelative(sample.createdAt)}</span>
                    </div>
                  ))}
                </div>
              ) : (
                <p className="field-hint">No measurements yet. Send a message or take a snip.</p>
              )}

              <details className="diag-logs">
                <summary>Server log · {diag.recentLogs.length} lines</summary>
                <pre>{diag.recentLogs.length > 0 ? diag.recentLogs.join("\n") : "No log lines yet."}</pre>
              </details>

              <div className="diag-actions">
                <button type="button" className="text-button" onClick={() => void loadDiagnostics()} disabled={diagLoading}>
                  <RefreshIcon size={14} />
                  {diagLoading ? "Refreshing…" : "Refresh"}
                </button>
                <button type="button" className="text-button" onClick={() => void copyReport()}>
                  <CopyIcon size={14} />
                  Copy report
                </button>
              </div>
            </>
          ) : null}
        </Section>

        <div className="settings-about">
          <LogoMark size={26} />
          <div>
            <strong>Bernard {appVersion}</strong>
            <span>
              A local companion. Gemma 4 through llama.cpp, no cloud in the loop.
            </span>
          </div>
        </div>
      </div>

      <footer className="settings-foot">
        <div className={`settings-foot-status${saveError ? " is-error" : dirty ? " is-dirty" : ""}`}>
          {saveError ? (
            <span>{saveError}</span>
          ) : dirty ? (
            <>
              <i aria-hidden />
              Unsaved changes
            </>
          ) : (
            <span>All changes saved</span>
          )}
        </div>
        <button type="button" className="ghost-button" onClick={requestBack}>
          Back
        </button>
        <button
          type="button"
          className="primary-button"
          onClick={() => void save()}
          disabled={!dirty || saving}
        >
          {saving ? "Saving…" : "Save changes"}
        </button>
      </footer>

      {confirmDiscard ? (
        <div className="discard-layer" role="alertdialog" aria-modal="true" aria-label="Discard changes">
          <div className="discard-card">
            <AlertIcon size={20} />
            <h2>Discard your changes?</h2>
            <p>Your edits to the model, prompt, hotkey, or appearance will not be saved.</p>
            <div className="discard-actions">
              <button type="button" className="ghost-button" onClick={() => setConfirmDiscard(false)}>
                Keep editing
              </button>
              <button type="button" className="danger-button" onClick={discard}>
                Discard changes
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}
