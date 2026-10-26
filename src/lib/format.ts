export function windowKind(): "avatar" | "panel" | "menu" | "overlay" {
  const params = new URLSearchParams(window.location.search);
  const value = params.get("window");
  if (value === "avatar" || value === "panel" || value === "menu" || value === "overlay") {
    return value;
  }
  return "panel";
}

export function overlayLabel(): string {
  const params = new URLSearchParams(window.location.search);
  return params.get("label") ?? "overlay-0";
}

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const exponent = Math.min(
    Math.floor(Math.log(bytes) / Math.log(1024)),
    units.length - 1,
  );
  const value = bytes / Math.pow(1024, exponent);
  const digits = value >= 100 || exponent === 0 ? 0 : value >= 10 ? 1 : 2;
  return `${value.toFixed(digits)} ${units[exponent]}`;
}

export function formatSpeed(bytesPerSecond: number): string {
  if (bytesPerSecond <= 0) return "—";
  return `${formatBytes(bytesPerSecond)}/s`;
}

export function formatDuration(ms: number): string {
  if (!Number.isFinite(ms) || ms <= 0) return "0 ms";
  if (ms < 1000) return `${Math.round(ms)} ms`;
  const seconds = ms / 1000;
  if (seconds < 60) return `${seconds.toFixed(seconds < 10 ? 1 : 0)} s`;
  const minutes = Math.floor(seconds / 60);
  const rest = Math.round(seconds % 60);
  if (minutes < 60) return `${minutes} min ${rest} s`;
  const hours = Math.floor(minutes / 60);
  return `${hours} h ${minutes % 60} min`;
}

export function formatClock(ms: number): string {
  const date = new Date(ms);
  return date.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
}

export function formatRelative(ms: number): string {
  const delta = Date.now() - ms;
  if (delta < 45_000) return "just now";
  if (delta < 3_600_000) return `${Math.max(1, Math.round(delta / 60_000))} min ago`;
  if (delta < 86_400_000) return `${Math.round(delta / 3_600_000)} hr ago`;
  if (delta < 7 * 86_400_000) return `${Math.round(delta / 86_400_000)} d ago`;
  return new Date(ms).toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

export function formatDateGroup(ms: number): string {
  const date = new Date(ms);
  const today = new Date();
  const startOfToday = new Date(today.getFullYear(), today.getMonth(), today.getDate()).getTime();
  if (ms >= startOfToday) return "Today";
  if (ms >= startOfToday - 86_400_000) return "Yesterday";
  if (ms >= startOfToday - 6 * 86_400_000) return "Previous 7 days";
  if (date.getFullYear() === today.getFullYear()) {
    return date.toLocaleDateString(undefined, { month: "long" });
  }
  return date.toLocaleDateString(undefined, { month: "short", year: "numeric" });
}

export function conciseTitle(text: string, limit = 52): string {
  const clean = text.replace(/\s+/g, " ").trim();
  if (!clean) return "Snip";
  return clean.length > limit ? `${clean.slice(0, limit - 1)}…` : clean;
}

export function fileStem(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] ?? path;
}

export function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

export function percent(downloaded: number, total: number): number {
  if (total <= 0) return 0;
  return clamp((downloaded / total) * 100, 0, 100);
}

export function messageOf(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "Bernard could not complete that request.";
}
