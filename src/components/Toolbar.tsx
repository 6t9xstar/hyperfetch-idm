import { matchesFilter, useStore } from "../store";
import type { Filter } from "../lib/types";
import { ClockIcon, FilmIcon, PauseIcon, PlayIcon, PlusIcon, RefreshIcon, SearchIcon } from "./Icons";

const TITLES: Record<Filter, string> = {
  all: "All downloads",
  downloading: "Downloading",
  queued: "Queued",
  paused: "Paused",
  finished: "Completed",
  failed: "Failed",
  video: "Video",
  audio: "Audio",
  documents: "Documents",
  archives: "Archives",
  programs: "Programs",
  images: "Images",
  other: "Other files",
};

export function Toolbar() {
  const filter = useStore((state) => state.filter);
  const downloads = useStore((state) => state.downloads);
  const search = useStore((state) => state.search);
  const setSearch = useStore((state) => state.setSearch);
  const openAdd = useStore((state) => state.openAdd);
  const setGrabberOpen = useStore((state) => state.setGrabberOpen);
  const setSchedulerOpen = useStore((state) => state.setSchedulerOpen);
  const pauseAll = useStore((state) => state.pauseAll);
  const resumeAll = useStore((state) => state.resumeAll);
  const refresh = useStore((state) => state.refresh);

  const count = downloads.filter((download) => matchesFilter(download, filter)).length;

  return (
    <header className="flex flex-wrap items-center gap-3 border-b border-white/[0.06] px-5 py-3">
      <div className="min-w-40 flex-1">
        <h1 className="text-[15px] font-semibold tracking-tight text-slate-100">{TITLES[filter]}</h1>
        <p className="text-[11px] text-slate-500">
          {count} item{count === 1 ? "" : "s"}
          {search ? ` matching “${search}”` : ""}
        </p>
      </div>

      <div className="relative order-3 w-full sm:order-none sm:w-64">
        <SearchIcon className="pointer-events-none absolute left-2.5 top-1/2 h-4 w-4 -translate-y-1/2 text-slate-500" />
        <input
          value={search}
          onChange={(event) => setSearch(event.target.value)}
          placeholder="Search downloads…"
          className="input h-9 pl-8 pr-3 text-[13px]"
        />
      </div>

      <div className="flex items-center gap-1.5">
        <button className="icon-btn" onClick={() => void refresh()} title="Refresh list">
          <RefreshIcon className="h-4 w-4" />
        </button>
        <button className="icon-btn" onClick={() => void pauseAll()} title="Pause all">
          <PauseIcon className="h-4 w-4" />
        </button>
        <button className="icon-btn" onClick={() => void resumeAll()} title="Resume all">
          <PlayIcon className="h-4 w-4" />
        </button>
        <button
          className="icon-btn"
          onClick={() => setSchedulerOpen(true)}
          title="Scheduler and queues"
        >
          <ClockIcon className="h-4 w-4 text-amber-400" />
        </button>
        <button
          className="btn h-9 px-2.5"
          onClick={() => setGrabberOpen(true)}
          title="Video and site grabber"
        >
          <FilmIcon className="h-4 w-4 text-accent-400" />
          <span className="hidden lg:inline">Grabber</span>
        </button>
        <button className="btn btn-primary h-9" onClick={() => openAdd()}>
          <PlusIcon className="h-4 w-4" />
          New download
        </button>
      </div>
    </header>
  );
}
