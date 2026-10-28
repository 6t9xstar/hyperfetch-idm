import type { MouseEvent } from "react";
import { matchesFilter, useStore } from "../store";
import type { DownloadSnapshot } from "../lib/types";
import {
  categoryOfFilename,
  formatBytes,
  formatEta,
  formatPercent,
  formatSpeed,
  isActive,
  progressOf,
  STATE_LABEL,
  STATE_STYLE,
  domainOf,
} from "../lib/format";
import { api } from "../lib/api";
import {
  CategoryIcon,
  DownloadIcon,
  ExternalIcon,
  PauseIcon,
  PlayIcon,
  PlusIcon,
  SearchIcon,
  TrashIcon,
  XIcon,
} from "./Icons";

const BAR_COLOR: Record<string, string> = {
  running: "bg-gradient-to-r from-accent-500 to-accent-300",
  probing: "bg-sky-400",
  completing: "bg-sky-400",
  paused: "bg-amber-400/80",
  error: "bg-rose-500/80",
  canceled: "bg-slate-500",
  done: "bg-emerald-500/60",
  queued: "bg-slate-600",
};

function DownloadRow({ download }: { download: DownloadSnapshot }) {
  const pause = useStore((state) => state.pause);
  const resume = useStore((state) => state.resume);
  const cancel = useStore((state) => state.cancel);
  const remove = useStore((state) => state.remove);
  const notify = useStore((state) => state.notify);

  const category = categoryOfFilename(download.filename);
  const active = isActive(download.state);
  const queued = download.state === "queued";
  const canResume =
    download.state === "paused" || download.state === "error" || download.state === "canceled";
  const percent = Math.round(progressOf(download) * 100);
  const indeterminate = download.state === "probing" || download.state === "completing";

  const openFile = async (event: MouseEvent) => {
    event.stopPropagation();
    try {
      await api.openPath(`${download.dir}/${download.filename}`);
    } catch (error) {
      notify(String(error), "error");
    }
  };

  return (
    <div
      className="hf-row group transition-colors hover:bg-white/[0.025]"
      onDoubleClick={() => {
        if (download.state === "done") {
          void api.openPath(`${download.dir}/${download.filename}`).catch((error) =>
            notify(String(error), "error"),
          );
        }
      }}
    >
      <div className="flex min-w-0 items-center gap-3">
        <span
          className={`flex h-9 w-9 shrink-0 items-center justify-center rounded-lg border ${
            active
              ? "border-accent-500/25 bg-accent-500/10 text-accent-300"
              : "border-white/[0.06] bg-white/[0.03] text-slate-400"
          }`}
        >
          <CategoryIcon category={category} className="h-4 w-4" />
        </span>
        <div className="min-w-0 flex-1">
          <p className="truncate text-[13px] font-medium text-slate-100" title={download.filename}>
            {download.filename || "Resolving filename…"}
          </p>
          <p className="mt-0.5 flex items-center gap-1.5 truncate text-[11px] text-slate-500">
            <span className="truncate">{domainOf(download.url)}</span>
            {download.accept_ranges && download.total_size > 0 ? (
              <span className="shrink-0 rounded bg-white/[0.06] px-1 text-[9px] uppercase tracking-wide text-slate-500">
                multi
              </span>
            ) : null}
            {download.error ? (
              <span className="truncate text-rose-400/80" title={download.error}>
                {download.error}
              </span>
            ) : null}
          </p>
          <div className="mt-1.5 h-1 w-full max-w-md overflow-hidden rounded-full bg-white/[0.06]">
            <div
              className={`h-full rounded-full transition-[width] duration-300 ${
                BAR_COLOR[download.state] ?? "bg-slate-600"
              } ${indeterminate ? "animate-pulse" : ""}`}
              style={{ width: indeterminate ? "100%" : `${Math.max(percent, 2)}%` }}
            />
          </div>
        </div>
      </div>

      <div className="text-right">
        <p className="font-mono text-[12px] text-slate-200">
          {active ? formatSpeed(download.speed) : "—"}
        </p>
        <p className="mt-0.5 text-[10px] text-slate-500">
          {active
            ? download.eta_secs != null
              ? `ETA ${formatEta(download.eta_secs)}`
              : "finalizing…"
            : download.active_connections > 0
              ? `${download.active_connections} conn`
              : ""}
        </p>
      </div>

      <div className="flex justify-start">
        <span
          className={`chip ${STATE_STYLE[download.state]}`}
          title={download.error ?? STATE_LABEL[download.state]}
        >
          <span className="h-1 w-1 rounded-full bg-current" />
          {STATE_LABEL[download.state]}
        </span>
      </div>

      <div className="text-right">
        <p className="font-mono text-[12px] text-slate-300">{formatPercent(download)}</p>
        <p className="mt-0.5 font-mono text-[10px] text-slate-500">
          {download.total_size
            ? formatBytes(download.total_size)
            : formatBytes(download.received)}
        </p>
      </div>

      <div className="flex items-center justify-end gap-0.5 opacity-60 transition-opacity group-hover:opacity-100">
        {active || queued ? (
          <button
            className="icon-btn"
            title="Pause"
            onClick={() => void pause(download.id)}
          >
            <PauseIcon className="h-4 w-4" />
          </button>
        ) : null}
        {canResume ? (
          <button
            className="icon-btn"
            title="Resume"
            onClick={() => void resume(download.id)}
          >
            <PlayIcon className="h-4 w-4" />
          </button>
        ) : null}
        {download.state === "done" ? (
          <button className="icon-btn" title="Open file" onClick={openFile}>
            <ExternalIcon className="h-4 w-4" />
          </button>
        ) : null}
        {active || queued ? (
          <button
            className="icon-btn"
            title="Cancel transfer"
            onClick={() => void cancel(download.id)}
          >
            <XIcon className="h-4 w-4" />
          </button>
        ) : null}
        <button
          className="icon-btn hover:text-rose-300"
          title="Remove from list (keeps file)"
          onClick={() => void remove(download.id, false)}
        >
          <TrashIcon className="h-4 w-4" />
        </button>
      </div>
    </div>
  );
}

function EmptyState() {
  const filter = useStore((state) => state.filter);
  const search = useStore((state) => state.search);
  const openAdd = useStore((state) => state.openAdd);

  return (
    <div className="flex flex-1 flex-col items-center justify-center px-6 py-20 text-center">
      <div className="flex h-16 w-16 items-center justify-center rounded-2xl border border-white/[0.07] bg-white/[0.03] text-slate-500">
        {search ? <SearchIcon className="h-7 w-7" /> : <DownloadIcon className="h-7 w-7" />}
      </div>
      <h2 className="mt-4 text-sm font-semibold text-slate-200">
        {search ? "No matching downloads" : filter === "all" ? "No downloads yet" : "Nothing here"}
      </h2>
      <p className="mt-1 max-w-xs text-xs leading-relaxed text-slate-500">
        {search
          ? "Try a different search term or clear the filter."
          : "Paste a URL to start a multi-connection download, or capture links straight from your browser."}
      </p>
      {!search ? (
        <button className="btn btn-primary mt-5 h-9" onClick={() => openAdd()}>
          <PlusIcon className="h-4 w-4" />
          New download
        </button>
      ) : null}
    </div>
  );
}

export function DownloadTable() {
  const downloads = useStore((state) => state.downloads);
  const filter = useStore((state) => state.filter);
  const search = useStore((state) => state.search);

  const normalized = search.trim().toLowerCase();
  const visible = downloads.filter((download) => {
    if (!matchesFilter(download, filter)) return false;
    if (!normalized) return true;
    return (
      download.filename.toLowerCase().includes(normalized) ||
      download.url.toLowerCase().includes(normalized)
    );
  });

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="hf-row sticky top-0 z-10 border-b border-white/[0.06] bg-ink-900/95 py-2 text-[10px] font-semibold uppercase tracking-[0.12em] text-slate-500 backdrop-blur">
        <span>Name</span>
        <span className="text-right">Speed</span>
        <span>Status</span>
        <span className="text-right">Progress</span>
        <span className="text-right">Actions</span>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto">
        {visible.length === 0 ? (
          <EmptyState />
        ) : (
          visible.map((download) => <DownloadRow key={download.id} download={download} />)
        )}
      </div>
      <footer className="flex items-center justify-between border-t border-white/[0.06] px-5 py-2 text-[10px] text-slate-600">
        <span>
          Showing {visible.length} of {downloads.length} downloads
        </span>
        <span className="hidden sm:inline">
          Double-click a completed item to open it · HyperFetch keeps partial files
          (<span className="font-mono">.hfpart</span>) safe across restarts
        </span>
      </footer>
    </div>
  );
}
