import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  BridgeInfo,
  CaptureAction,
  CaptureOffer,
  ClipboardOffer,
  DiscoveredLink,
  DownloadSnapshot,
  DownloadSpec,
  EngineEvent,
  EngineStats,
  ProbeResult,
  Settings,
} from "./types";

export const api = {
  probe: (url: string, headers?: Record<string, string>) =>
    invoke<ProbeResult>("probe_url", { url, headers: headers ?? null }),
  add: (spec: DownloadSpec) => invoke<DownloadSnapshot>("add_download", { spec }),
  list: () => invoke<DownloadSnapshot[]>("list_downloads"),
  get: (id: string) => invoke<DownloadSnapshot>("get_download", { id }),
  pause: (id: string) => invoke<void>("pause_download", { id }),
  resume: (id: string) => invoke<void>("resume_download", { id }),
  cancel: (id: string) => invoke<void>("cancel_download", { id }),
  remove: (id: string, deleteFile: boolean) =>
    invoke<void>("remove_download", { id, deleteFile }),
  pauseAll: () => invoke<void>("pause_all"),
  resumeAll: () => invoke<void>("resume_all"),
  clearCompleted: () => invoke<void>("clear_completed"),
  getSettings: () => invoke<Settings>("get_settings"),
  updateSettings: (settings: Settings) =>
    invoke<Settings>("update_settings", { settings }),
  stats: () => invoke<EngineStats>("get_stats"),
  bridgeInfo: () => invoke<BridgeInfo>("bridge_info"),
  openFolder: (path: string) => invoke<void>("open_folder", { path }),
  openPath: (path: string) => invoke<void>("open_path", { path }),
  crawl: (url: string, maxDepth: number, maxPages: number) =>
    invoke<DiscoveredLink[]>("crawl_site", { url, maxDepth, maxPages }),
  startHls: (url: string, filename?: string | null) =>
    invoke<string>("start_hls", { url, filename: filename ?? null, headers: null }),
  listCaptures: () => invoke<CaptureOffer[]>("list_captures"),
  acceptCapture: (id: string) =>
    invoke<CaptureAction>("accept_capture", { id }),
  dismissCapture: (id: string) => invoke<void>("dismiss_capture", { id }),
  listClipboardOffers: () => invoke<ClipboardOffer[]>("list_clipboard_offers"),
  acceptClipboardOffer: (id: string) =>
    invoke<CaptureAction>("accept_clipboard_offer", { id }),
  dismissClipboardOffer: (id: string) =>
    invoke<void>("dismiss_clipboard_offer", { id }),
};

export function onEngineEvent(handler: (event: EngineEvent) => void): Promise<UnlistenFn> {
  return listen<EngineEvent>("engine://event", (event) => handler(event.payload));
}

export function onCaptureOffer(handler: (offer: CaptureOffer) => void): Promise<UnlistenFn> {
  return listen<CaptureOffer>("capture://offer", (event) => handler(event.payload));
}

export function onClipboardOffer(
  handler: (offer: ClipboardOffer) => void,
): Promise<UnlistenFn> {
  return listen<ClipboardOffer>("clipboard://offer", (event) => handler(event.payload));
}

export interface GrabberProgress {
  id: string;
  completed_segments: number;
  total_segments: number;
  bytes: number;
}

export interface GrabberDone {
  id: string;
  path: string;
  bytes: number;
  segments: number;
}

export interface GrabberError {
  id: string;
  message: string;
}

export function onGrabberProgress(
  handler: (progress: GrabberProgress) => void,
): Promise<UnlistenFn> {
  return listen<GrabberProgress>("grabber://progress", (event) => handler(event.payload));
}

export function onGrabberDone(handler: (done: GrabberDone) => void): Promise<UnlistenFn> {
  return listen<GrabberDone>("grabber://done", (event) => handler(event.payload));
}

export function onGrabberError(handler: (error: GrabberError) => void): Promise<UnlistenFn> {
  return listen<GrabberError>("grabber://error", (event) => handler(event.payload));
}
