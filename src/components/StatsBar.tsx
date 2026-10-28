import { useStore } from "../store";
import { formatBytes, formatSpeed } from "../lib/format";
import { DownloadIcon, PulseIcon, ZapIcon } from "./Icons";

export function StatsBar() {
  const stats = useStore((state) => state.stats);
  const downloads = useStore((state) => state.downloads);
  const grabberTasks = useStore((state) => state.grabberTasks);

  const runningGrabbers = grabberTasks.filter((task) => task.status === "running").length;
  const totalReceived = downloads.reduce((sum, download) => sum + download.received, 0);

  return (
    <div className="grid gap-px overflow-hidden border-b border-white/[0.06] bg-white/[0.04] sm:grid-cols-4">
      <div className="flex items-center gap-3 bg-ink-900/80 px-5 py-3">
        <span className="flex h-8 w-8 items-center justify-center rounded-lg bg-accent-500/10 text-accent-400">
          <ZapIcon className="h-4 w-4" />
        </span>
        <div>
          <p className="text-[10px] uppercase tracking-[0.14em] text-slate-500">Total speed</p>
          <p className="font-mono text-sm font-semibold text-slate-100">
            {formatSpeed(stats.speed)}
          </p>
        </div>
      </div>
      <div className="flex items-center gap-3 bg-ink-900/80 px-5 py-3">
        <span className="flex h-8 w-8 items-center justify-center rounded-lg bg-sky-500/10 text-sky-400">
          <PulseIcon className="h-4 w-4" />
        </span>
        <div>
          <p className="text-[10px] uppercase tracking-[0.14em] text-slate-500">Active</p>
          <p className="font-mono text-sm font-semibold text-slate-100">
            {stats.active}
            {runningGrabbers > 0 ? (
              <span className="ml-1 text-[11px] font-normal text-accent-400">
                +{runningGrabbers} video
              </span>
            ) : null}
          </p>
        </div>
      </div>
      <div className="flex items-center gap-3 bg-ink-900/80 px-5 py-3">
        <span className="flex h-8 w-8 items-center justify-center rounded-lg bg-amber-500/10 text-amber-400">
          <DownloadIcon className="h-4 w-4" />
        </span>
        <div>
          <p className="text-[10px] uppercase tracking-[0.14em] text-slate-500">Queued</p>
          <p className="font-mono text-sm font-semibold text-slate-100">{stats.queued}</p>
        </div>
      </div>
      <div className="hidden items-center gap-3 bg-ink-900/80 px-5 py-3 sm:flex">
        <div>
          <p className="text-[10px] uppercase tracking-[0.14em] text-slate-500">Downloaded</p>
          <p className="font-mono text-sm font-semibold text-slate-100">
            {formatBytes(totalReceived)}
          </p>
        </div>
      </div>
    </div>
  );
}
