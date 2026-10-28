import type { DownloadSnapshot, DownloadState } from "./types";

const UNITS = ["B", "KB", "MB", "GB", "TB"];

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const precision = value >= 100 || unit === 0 ? 0 : value >= 10 ? 1 : 2;
  return `${value.toFixed(precision)} ${UNITS[unit]}`;
}

export function formatSpeed(bytes: number): string {
  if (!bytes) return "—";
  return `${formatBytes(bytes)}/s`;
}

export function formatEta(seconds: number | null | undefined): string {
  if (seconds == null || !Number.isFinite(seconds) || seconds <= 0) return "—";
  if (seconds < 60) return `${Math.round(seconds)}s`;
  const minutes = Math.floor(seconds / 60);
  const rest = Math.round(seconds % 60);
  if (minutes < 60) return `${minutes}m ${rest}s`;
  const hours = Math.floor(minutes / 60);
  return `${hours}h ${minutes % 60}m`;
}

export function progressOf(download: DownloadSnapshot): number {
  if (!download.total_size) return 0;
  return Math.min(1, download.received / download.total_size);
}

export function formatPercent(download: DownloadSnapshot): string {
  return `${Math.floor(progressOf(download) * 100)}%`;
}

export function domainOf(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return url;
  }
}

export function extensionOf(filename: string): string {
  const index = filename.lastIndexOf(".");
  return index > 0 ? filename.slice(index + 1).toLowerCase() : "";
}

const CATEGORY_MAP: Record<string, string> = {
  mp4: "video",
  mkv: "video",
  avi: "video",
  mov: "video",
  wmv: "video",
  flv: "video",
  webm: "video",
  m4v: "video",
  mpg: "video",
  mpeg: "video",
  ts: "video",
  m3u8: "video",
  mpd: "video",
  mp3: "audio",
  wav: "audio",
  flac: "audio",
  aac: "audio",
  ogg: "audio",
  m4a: "audio",
  opus: "audio",
  wma: "audio",
  pdf: "documents",
  doc: "documents",
  docx: "documents",
  xls: "documents",
  xlsx: "documents",
  ppt: "documents",
  pptx: "documents",
  txt: "documents",
  rtf: "documents",
  epub: "documents",
  mobi: "documents",
  csv: "documents",
  md: "documents",
  zip: "archives",
  rar: "archives",
  "7z": "archives",
  tar: "archives",
  gz: "archives",
  bz2: "archives",
  xz: "archives",
  iso: "archives",
  zst: "archives",
  exe: "programs",
  msi: "programs",
  msix: "programs",
  appx: "programs",
  apk: "programs",
  dmg: "programs",
  deb: "programs",
  rpm: "programs",
  pkg: "programs",
  jpg: "images",
  jpeg: "images",
  png: "images",
  gif: "images",
  bmp: "images",
  webp: "images",
  svg: "images",
  tiff: "images",
  heic: "images",
};

export function categoryOfFilename(filename: string): string {
  return CATEGORY_MAP[extensionOf(filename)] ?? "other";
}

export const STATE_LABEL: Record<DownloadState, string> = {
  queued: "Queued",
  probing: "Connecting",
  running: "Downloading",
  paused: "Paused",
  completing: "Finalizing",
  done: "Completed",
  error: "Failed",
  canceled: "Canceled",
};

export const STATE_STYLE: Record<DownloadState, string> = {
  queued: "bg-slate-500/15 text-slate-300 border-slate-400/20",
  probing: "bg-sky-500/15 text-sky-300 border-sky-400/20",
  running: "bg-accent-500/15 text-accent-300 border-accent-500/30",
  paused: "bg-amber-500/15 text-amber-300 border-amber-400/20",
  completing: "bg-sky-500/15 text-sky-300 border-sky-400/20",
  done: "bg-emerald-500/15 text-emerald-300 border-emerald-400/20",
  error: "bg-rose-500/15 text-rose-300 border-rose-400/20",
  canceled: "bg-slate-500/15 text-slate-400 border-slate-500/20",
};

export function isActive(state: DownloadState): boolean {
  return state === "running" || state === "probing" || state === "completing";
}

export function relativeTime(timestamp: number): string {
  const seconds = Math.max(0, (Date.now() - timestamp) / 1000);
  if (seconds < 60) return "just now";
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
  return `${Math.floor(seconds / 86400)}d ago`;
}
