import { useMemo } from "react";
import { matchesFilter, useStore } from "../store";
import type { Filter } from "../lib/types";
import {
  ArchiveIcon,
  DownloadIcon,
  FileIcon,
  FilmIcon,
  FolderIcon,
  ImageIcon,
  Logo,
  MusicIcon,
  PauseIcon,
  ProgramIcon,
  PulseIcon,
  SettingsIcon,
  XIcon,
} from "./Icons";

interface NavItem {
  key: Filter;
  label: string;
  icon: (props: { className?: string }) => JSX.Element;
}

const MAIN: NavItem[] = [
  { key: "all", label: "All downloads", icon: DownloadIcon },
  { key: "downloading", label: "Downloading", icon: PulseIcon },
  { key: "queued", label: "Queued", icon: PulseIcon },
  { key: "paused", label: "Paused", icon: PauseIcon },
  { key: "finished", label: "Completed", icon: DownloadIcon },
  { key: "failed", label: "Failed", icon: XIcon },
];

const CATEGORIES: NavItem[] = [
  { key: "video", label: "Video", icon: FilmIcon },
  { key: "audio", label: "Audio", icon: MusicIcon },
  { key: "documents", label: "Documents", icon: FileIcon },
  { key: "archives", label: "Archives", icon: ArchiveIcon },
  { key: "programs", label: "Programs", icon: ProgramIcon },
  { key: "images", label: "Images", icon: ImageIcon },
];

export function Sidebar() {
  const downloads = useStore((state) => state.downloads);
  const filter = useStore((state) => state.filter);
  const setFilter = useStore((state) => state.setFilter);
  const bridge = useStore((state) => state.bridge);
  const setSettingsOpen = useStore((state) => state.setSettingsOpen);
  const grabberTasks = useStore((state) => state.grabberTasks);
  const runningGrabbers = useMemo(
    () => grabberTasks.filter((task) => task.status === "running").length,
    [grabberTasks],
  );

  const countByFilter = useMemo(() => {
    const counts = new Map<Filter, number>();
    for (const download of downloads) {
      for (const item of MAIN) {
        if (matchesFilter(download, item.key)) {
          counts.set(item.key, (counts.get(item.key) ?? 0) + 1);
        }
      }
      for (const item of CATEGORIES) {
        if (matchesFilter(download, item.key)) {
          counts.set(item.key, (counts.get(item.key) ?? 0) + 1);
        }
      }
    }
    return counts;
  }, [downloads]);

  const renderItem = (item: NavItem) => {
    const Icon = item.icon;
    const count = countByFilter.get(item.key) ?? 0;
    const active = filter === item.key;
    return (
      <button
        key={item.key}
        onClick={() => setFilter(item.key)}
        className={`group flex w-full items-center gap-2.5 rounded-lg px-2.5 py-2 text-left text-[13px] transition ${
          active
            ? "bg-accent-500/10 font-medium text-accent-300"
            : "text-slate-400 hover:bg-white/[0.05] hover:text-slate-200"
        }`}
      >
        <Icon className={`h-4 w-4 ${active ? "text-accent-400" : "text-slate-500 group-hover:text-slate-300"}`} />
        <span className="flex-1 truncate">{item.label}</span>
        {count > 0 ? (
          <span
            className={`text-[11px] tabular-nums ${
              active ? "text-accent-400/80" : "text-slate-600"
            }`}
          >
            {count}
          </span>
        ) : null}
      </button>
    );
  };

  return (
    <aside className="flex w-56 shrink-0 flex-col border-r border-white/[0.06] bg-ink-900/60">
      <div className="flex items-center gap-2.5 px-4 pb-4 pt-5">
        <Logo className="h-8 w-8" />
        <div className="min-w-0">
          <p className="truncate text-sm font-semibold tracking-tight text-slate-100">HyperFetch</p>
          <p className="text-[10px] uppercase tracking-[0.18em] text-slate-500">Download manager</p>
        </div>
      </div>

      <nav className="flex-1 space-y-6 overflow-y-auto px-2.5 pb-4">
        <div className="space-y-0.5">{MAIN.map(renderItem)}</div>
        <div>
          <p className="section-title px-2.5 pb-1.5">Categories</p>
          <div className="space-y-0.5">{CATEGORIES.map(renderItem)}</div>
        </div>
      </nav>

      <div className="space-y-2 border-t border-white/[0.06] p-2.5">
        <div className="rounded-lg border border-white/[0.05] bg-ink-850/60 px-2.5 py-2 text-[11px]">
          <div className="flex items-center gap-1.5">
            <span
              className={`h-1.5 w-1.5 rounded-full ${
                bridge ? "bg-accent-400 shadow-[0_0_8px_rgba(62,230,164,.8)]" : "bg-slate-600"
              }`}
            />
            <span className="text-slate-400">
              {bridge ? "Browser bridge online" : "Bridge offline"}
            </span>
          </div>
          <p className="mt-1 pl-3 font-mono text-[10px] text-slate-600">
            {bridge ? `127.0.0.1:${bridge.port}` : "not running"}
          </p>
        </div>
        <button
          onClick={() => setSettingsOpen(true)}
          className="flex w-full items-center gap-2.5 rounded-lg px-2.5 py-2 text-[13px] text-slate-400 transition hover:bg-white/[0.05] hover:text-slate-200"
        >
          <SettingsIcon className="h-4 w-4 text-slate-500" />
          <span className="flex-1 text-left">Settings</span>
        </button>
        <div className="flex items-center justify-between px-2.5 text-[10px] text-slate-600">
          <span className="flex items-center gap-1">
            <FolderIcon className="h-3 w-3" />
            {runningGrabbers > 0 ? `${runningGrabbers} video task${runningGrabbers > 1 ? "s" : ""}` : "v0.1.0 · MIT"}
          </span>
        </div>
      </div>
    </aside>
  );
}
