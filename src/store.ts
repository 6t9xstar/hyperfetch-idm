import { create } from "zustand";
import type { UnlistenFn } from "@tauri-apps/api/event";
import {
  api,
  onCaptureOffer,
  onClipboardOffer,
  onEngineEvent,
  onGrabberDone,
  onGrabberError,
  onGrabberProgress,
} from "./lib/api";
import type {
  BridgeInfo,
  CaptureOffer,
  ClipboardOffer,
  DownloadSnapshot,
  EngineEvent,
  EngineStats,
  Filter,
  Settings,
} from "./lib/types";
import { categoryOfFilename, isActive } from "./lib/format";

export type ToastKind = "info" | "success" | "error";

export interface Toast {
  id: number;
  message: string;
  kind: ToastKind;
}

export interface GrabberTask {
  id: string;
  label: string;
  done: number;
  total: number;
  bytes: number;
  status: "running" | "done" | "error";
  path?: string;
  message?: string;
}

interface Store {
  initialized: boolean;
  downloads: DownloadSnapshot[];
  settings: Settings | null;
  stats: EngineStats;
  bridge: BridgeInfo | null;
  captures: CaptureOffer[];
  grabberTasks: GrabberTask[];
  toasts: Toast[];
  filter: Filter;
  search: string;
  addOpen: boolean;
  addUrl: string;
  settingsOpen: boolean;
  grabberOpen: boolean;
  capturesOpen: boolean;
  progressDialogId: string | null;
  schedulerOpen: boolean;
  propertiesDownload: DownloadSnapshot | null;
  openProgressDialog: (id: string) => void;
  closeProgressDialog: () => void;
  setSchedulerOpen: (open: boolean) => void;
  openProperties: (download: DownloadSnapshot) => void;
  closeProperties: () => void;
  init: () => Promise<void>;
  refresh: () => Promise<void>;
  refreshCaptures: () => Promise<void>;
  applyEvent: (event: EngineEvent) => void;
  notify: (message: string, kind?: ToastKind) => void;
  dismissToast: (id: number) => void;
  setFilter: (filter: Filter) => void;
  setSearch: (search: string) => void;
  openAdd: (url?: string) => void;
  closeAdd: () => void;
  setSettingsOpen: (open: boolean) => void;
  setGrabberOpen: (open: boolean) => void;
  setCapturesOpen: (open: boolean) => void;
  pause: (id: string) => Promise<void>;
  resume: (id: string) => Promise<void>;
  cancel: (id: string) => Promise<void>;
  remove: (id: string, deleteFile: boolean) => Promise<void>;
  pauseAll: () => Promise<void>;
  resumeAll: () => Promise<void>;
  clearCompleted: () => Promise<void>;
  saveSettings: (settings: Settings) => Promise<boolean>;
  acceptCapture: (id: string) => Promise<void>;
  dismissCapture: (id: string) => Promise<void>;
  trackGrabber: (id: string, label: string) => void;
  clearFinishedGrabbers: () => void;
}

let started = false;
let toastId = 0;

function withSource(
  offer: CaptureOffer | ClipboardOffer,
  source: "browser" | "clipboard",
): CaptureOffer {
  return {
    ...offer,
    referer: "referer" in offer ? offer.referer : null,
    cookie: "cookie" in offer ? offer.cookie : null,
    user_agent: "user_agent" in offer ? offer.user_agent : null,
    page_url: "page_url" in offer ? offer.page_url : null,
    source,
    auto: "auto" in offer ? offer.auto : false,
  };
}

function sortDownloads(downloads: DownloadSnapshot[]): DownloadSnapshot[] {
  return [...downloads].sort((a, b) => b.created_at - a.created_at);
}

function deriveStats(downloads: DownloadSnapshot[]): EngineStats {
  let speed = 0;
  let active = 0;
  let queued = 0;
  for (const download of downloads) {
    speed += download.speed;
    if (isActive(download.state)) active += 1;
    if (download.state === "queued") queued += 1;
  }
  return { speed, active, queued };
}

export function matchesFilter(download: DownloadSnapshot, filter: Filter): boolean {
  switch (filter) {
    case "all":
      return true;
    case "downloading":
      return isActive(download.state);
    case "queued":
      return download.state === "queued";
    case "paused":
      return download.state === "paused";
    case "finished":
      return download.state === "done";
    case "failed":
      return download.state === "error" || download.state === "canceled";
    default:
      return categoryOfFilename(download.filename) === filter;
  }
}

export const useStore = create<Store>()((set, get) => ({
  initialized: false,
  downloads: [],
  settings: null,
  stats: { active: 0, queued: 0, speed: 0 },
  bridge: null,
  captures: [],
  grabberTasks: [],
  toasts: [],
  filter: "all",
  search: "",
  addOpen: false,
  addUrl: "",
  settingsOpen: false,
  grabberOpen: false,
  capturesOpen: false,
  progressDialogId: null,
  schedulerOpen: false,
  propertiesDownload: null,

  openProgressDialog: (id) => set({ progressDialogId: id }),
  closeProgressDialog: () => set({ progressDialogId: null }),
  setSchedulerOpen: (open) => set({ schedulerOpen: open }),
  openProperties: (download) => set({ propertiesDownload: download }),
  closeProperties: () => set({ propertiesDownload: null }),

  init: async () => {
    if (started) return;
    started = true;
    const unlisteners: UnlistenFn[] = [];
    try {
      const [downloads, settings, captures, clipboard, bridge] = await Promise.all([
        api.list(),
        api.getSettings(),
        api.listCaptures(),
        api.listClipboardOffers(),
        api.bridgeInfo(),
      ]);
      set({
        downloads: sortDownloads(downloads),
        settings,
        captures: [
          ...captures.map((offer) => withSource(offer, "browser")),
          ...clipboard.map((offer) => withSource(offer, "clipboard")),
        ],
        bridge,
        stats: deriveStats(downloads),
        initialized: true,
      });

      unlisteners.push(await onEngineEvent((event) => get().applyEvent(event)));
      unlisteners.push(
        await onCaptureOffer((offer) => {
          const normalized = withSource(offer, "browser");
          set((state) => ({
            captures: [...state.captures.filter((item) => item.id !== offer.id), normalized],
            capturesOpen: true,
          }));
        }),
      );
      unlisteners.push(
        await onClipboardOffer((offer) => {
          if (offer.auto) {
            get().notify(`Added ${offer.filename} from clipboard`, "success");
            return;
          }
          const normalized = withSource(offer, "clipboard");
          set((state) => ({
            captures: [...state.captures.filter((item) => item.id !== offer.id), normalized],
            capturesOpen: true,
          }));
        }),
      );
      unlisteners.push(
        await onGrabberProgress((progress) => {
          set((state) => ({
            grabberTasks: state.grabberTasks.map((task) =>
              task.id === progress.id
                ? {
                    ...task,
                    done: progress.completed_segments,
                    total: progress.total_segments,
                    bytes: progress.bytes,
                    status: "running",
                  }
                : task,
            ),
          }));
        }),
      );
      unlisteners.push(
        await onGrabberDone((done) => {
          set((state) => ({
            grabberTasks: state.grabberTasks.map((task) =>
              task.id === done.id
                ? { ...task, status: "done", bytes: done.bytes, path: done.path, total: task.total }
                : task,
            ),
          }));
          get().notify(`Video saved to ${done.path}`, "success");
        }),
      );
      unlisteners.push(
        await onGrabberError((error) => {
          set((state) => ({
            grabberTasks: state.grabberTasks.map((task) =>
              task.id === error.id
                ? { ...task, status: "error", message: error.message }
                : task,
            ),
          }));
          get().notify(`Grabber failed: ${error.message}`, "error");
        }),
      );

      window.addEventListener("beforeunload", () => {
        unlisteners.forEach((unlisten) => unlisten());
      });
    } catch (error) {
      started = false;
      set({ initialized: true });
      get().notify(`Failed to initialize: ${String(error)}`, "error");
    }
  },

  refresh: async () => {
    try {
      const downloads = await api.list();
      set({ downloads: sortDownloads(downloads), stats: deriveStats(downloads) });
    } catch (error) {
      get().notify(String(error), "error");
    }
  },

  refreshCaptures: async () => {
    try {
      const [captures, clipboard] = await Promise.all([
        api.listCaptures(),
        api.listClipboardOffers(),
      ]);
      set({
        captures: [
          ...captures.map((offer) => withSource(offer, "browser")),
          ...clipboard.map((offer) => withSource(offer, "clipboard")),
        ],
      });
    } catch {
      /* ignore */
    }
  },

  applyEvent: (event) => {
    if (event.type === "added") {
      const download = event.download;
      set((state) => {
        const rest = state.downloads.filter((item) => item.id !== download.id);
        const downloads = sortDownloads([download, ...rest]);
        return { downloads, stats: deriveStats(downloads) };
      });
    } else if (event.type === "updated") {
      const download = event.download;
      set((state) => {
        const downloads = state.downloads.map((item) =>
          item.id === download.id ? download : item,
        );
        return { downloads, stats: deriveStats(downloads) };
      });
    } else if (event.type === "progress") {
      const progress = event.progress;
      set((state) => {
        const downloads = state.downloads.map((item) =>
          item.id === progress.id
            ? {
                ...item,
                received: progress.received,
                total_size: progress.total || item.total_size,
                speed: progress.speed,
                eta_secs: progress.eta_secs,
                active_connections: progress.active_connections,
              }
            : item,
        );
        return { downloads, stats: deriveStats(downloads) };
      });
    } else if (event.type === "removed") {
      set((state) => {
        const downloads = state.downloads.filter((item) => item.id !== event.id);
        return { downloads, stats: deriveStats(downloads) };
      });
    } else if (event.type === "queue") {
      set((state) => ({
        stats: { ...state.stats, active: event.active, queued: event.pending },
      }));
    }
  },

  notify: (message, kind = "info") => {
    const id = ++toastId;
    set((state) => ({ toasts: [...state.toasts.slice(-3), { id, message, kind }] }));
    window.setTimeout(() => get().dismissToast(id), 5000);
  },

  dismissToast: (id) =>
    set((state) => ({ toasts: state.toasts.filter((toast) => toast.id !== id) })),

  setFilter: (filter) => set({ filter }),
  setSearch: (search) => set({ search }),
  openAdd: (url) => set({ addOpen: true, addUrl: url ?? "" }),
  closeAdd: () => set({ addOpen: false, addUrl: "" }),
  setSettingsOpen: (settingsOpen) => set({ settingsOpen }),
  setGrabberOpen: (grabberOpen) => set({ grabberOpen }),
  setCapturesOpen: (capturesOpen) => set({ capturesOpen }),

  pause: async (id) => {
    try {
      await api.pause(id);
    } catch (error) {
      get().notify(String(error), "error");
    }
  },

  resume: async (id) => {
    try {
      await api.resume(id);
    } catch (error) {
      get().notify(String(error), "error");
    }
  },

  cancel: async (id) => {
    try {
      await api.cancel(id);
    } catch (error) {
      get().notify(String(error), "error");
    }
  },

  remove: async (id, deleteFile) => {
    try {
      await api.remove(id, deleteFile);
    } catch (error) {
      get().notify(String(error), "error");
    }
  },

  pauseAll: async () => {
    try {
      await api.pauseAll();
      get().notify("All transfers paused");
    } catch (error) {
      get().notify(String(error), "error");
    }
  },

  resumeAll: async () => {
    try {
      await api.resumeAll();
    } catch (error) {
      get().notify(String(error), "error");
    }
  },

  clearCompleted: async () => {
    try {
      await api.clearCompleted();
      await get().refresh();
      get().notify("Completed downloads cleared", "success");
    } catch (error) {
      get().notify(String(error), "error");
    }
  },

  saveSettings: async (settings) => {
    try {
      const updated = await api.updateSettings(settings);
      set({ settings: updated });
      get().notify("Settings saved", "success");
      return true;
    } catch (error) {
      get().notify(String(error), "error");
      return false;
    }
  },

  acceptCapture: async (id) => {
    try {
      const offer = get().captures.find((item) => item.id === id);
      const action =
        offer?.source === "clipboard"
          ? await api.acceptClipboardOffer(id)
          : await api.acceptCapture(id);
      set((state) => ({ captures: state.captures.filter((item) => item.id !== id) }));
      if (action.action === "grabber") {
        get().trackGrabber(action.id, offer?.filename ?? "HLS video");
        get().notify("HLS download started", "success");
      } else {
        get().notify("Download added", "success");
        await get().refresh();
      }
    } catch (error) {
      get().notify(String(error), "error");
    }
  },

  dismissCapture: async (id) => {
    try {
      const offer = get().captures.find((item) => item.id === id);
      if (offer?.source === "clipboard") {
        await api.dismissClipboardOffer(id);
      } else {
        await api.dismissCapture(id);
      }
      set((state) => ({ captures: state.captures.filter((offer) => offer.id !== id) }));
    } catch (error) {
      get().notify(String(error), "error");
    }
  },

  trackGrabber: (id, label) =>
    set((state) => ({
      grabberTasks: [
        ...state.grabberTasks.filter((task) => task.id !== id),
        { id, label, done: 0, total: 0, bytes: 0, status: "running" },
      ],
    })),

  clearFinishedGrabbers: () =>
    set((state) => ({
      grabberTasks: state.grabberTasks.filter((task) => task.status === "running"),
    })),
}));
