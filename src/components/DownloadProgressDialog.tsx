import { useEffect, useState } from "react";
import { useStore } from "../store";
import { api } from "../lib/api";
import type { RangeInfo } from "../lib/types";
import {
  formatBytes,
  formatEta,
  formatPercent,
  formatSpeed,
  isActive,
  progressOf,
  STATE_LABEL,
} from "../lib/format";
import {
  DownloadIcon,
  FolderIcon,
  PauseIcon,
  PlayIcon,
  PulseIcon,
  SpeedometerIcon,
  XIcon,
} from "./Icons";

export function DownloadProgressDialog() {
  const progressDialogId = useStore((state) => state.progressDialogId);
  const closeProgressDialog = useStore((state) => state.closeProgressDialog);
  const downloads = useStore((state) => state.downloads);
  const pause = useStore((state) => state.pause);
  const resume = useStore((state) => state.resume);
  const cancel = useStore((state) => state.cancel);
  const notify = useStore((state) => state.notify);
  const theme = useStore((state) => state.settings?.theme ?? "modern");
  const isClassic = theme === "classic";

  const download = downloads.find((item) => item.id === progressDialogId);
  const [ranges, setRanges] = useState<RangeInfo[]>([]);
  const [speedLimit, setSpeedLimit] = useState<number>(0);
  const [speedLimitEnabled, setSpeedLimitEnabled] = useState(false);

  useEffect(() => {
    if (!progressDialogId) {
      setRanges([]);
      return;
    }

    let active = true;
    const fetchRanges = async () => {
      try {
        const data = await api.getRanges(progressDialogId);
        if (active) setRanges(data);
      } catch {
        /* ignore */
      }
    };

    void fetchRanges();
    const interval = setInterval(fetchRanges, 800);
    return () => {
      active = false;
      clearInterval(interval);
    };
  }, [progressDialogId]);

  if (!progressDialogId || !download) return null;

  const active = isActive(download.state);
  const percent = Math.round(progressOf(download) * 100);
  const eta = active && download.eta_secs != null ? formatEta(download.eta_secs) : "—";

  const openFile = async () => {
    try {
      await api.openPath(`${download.dir}/${download.filename}`);
    } catch (error) {
      notify(String(error), "error");
    }
  };

  const openFolder = async () => {
    try {
      await api.openFolder(download.dir);
    } catch (error) {
      notify(String(error), "error");
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4 backdrop-blur-sm"
      onClick={closeProgressDialog}
    >
      <div
        className={
          isClassic
            ? "w-full max-w-2xl rounded border border-[#7a786c] bg-[#efede3] text-[#1a1a1a] shadow-2xl font-sans text-xs"
            : "w-full max-w-2xl rounded-2xl border border-white/10 bg-ink-900 p-6 text-slate-100 shadow-2xl"
        }
        onClick={(event) => event.stopPropagation()}
      >
        {/* Title bar */}
        <div
          className={
            isClassic
              ? "flex items-center justify-between border-b border-[#9c9a8e] bg-gradient-to-r from-[#0a3d91] to-[#1c5f9a] px-3 py-1.5 text-white font-bold"
              : "flex items-center justify-between border-b border-white/10 pb-4"
          }
        >
          <div className="flex items-center gap-2 truncate">
            <DownloadIcon className="h-4 w-4 shrink-0 text-emerald-400" />
            <span className="truncate">
              Download status: {download.filename || "Downloading file…"}
            </span>
          </div>
          <button
            onClick={closeProgressDialog}
            className={
              isClassic
                ? "flex h-4 w-4 items-center justify-center rounded bg-[#c53929] hover:bg-rose-600 text-white text-[10px] leading-none"
                : "text-slate-400 hover:text-white"
            }
          >
            ✕
          </button>
        </div>

        {/* Body content */}
        <div className={isClassic ? "p-4 space-y-3.5" : "mt-4 space-y-4"}>
          {/* Main Info Grid */}
          <div
            className={
              isClassic
                ? "grid grid-cols-2 gap-x-4 gap-y-1.5 border border-[#b8b5a7] bg-[#f8f7f2] p-2.5 rounded text-[11px]"
                : "grid grid-cols-2 gap-3 rounded-xl border border-white/5 bg-ink-950/60 p-4 text-xs"
            }
          >
            <div>
              <span className={isClassic ? "text-[#555348] font-semibold" : "text-slate-400"}>
                Status:
              </span>{" "}
              <span
                className={`font-semibold ${
                  active
                    ? "text-emerald-600 dark:text-emerald-400"
                    : download.state === "done"
                    ? "text-blue-600 dark:text-blue-400"
                    : "text-amber-600 dark:text-amber-400"
                }`}
              >
                {STATE_LABEL[download.state]}
              </span>
            </div>
            <div>
              <span className={isClassic ? "text-[#555348] font-semibold" : "text-slate-400"}>
                Transfer rate:
              </span>{" "}
              <span className="font-semibold text-emerald-600 dark:text-emerald-400">
                {active ? formatSpeed(download.speed) : "—"}
              </span>
            </div>
            <div>
              <span className={isClassic ? "text-[#555348] font-semibold" : "text-slate-400"}>
                File size:
              </span>{" "}
              <span>{download.total_size > 0 ? formatBytes(download.total_size) : "Unknown"}</span>
            </div>
            <div>
              <span className={isClassic ? "text-[#555348] font-semibold" : "text-slate-400"}>
                Time left:
              </span>{" "}
              <span>{eta}</span>
            </div>
            <div>
              <span className={isClassic ? "text-[#555348] font-semibold" : "text-slate-400"}>
                Downloaded:
              </span>{" "}
              <span className="font-semibold">
                {formatBytes(download.received)} ({formatPercent(download)})
              </span>
            </div>
            <div>
              <span className={isClassic ? "text-[#555348] font-semibold" : "text-slate-400"}>
                Resume capability:
              </span>{" "}
              <span
                className={
                  download.accept_ranges
                    ? "text-emerald-600 dark:text-emerald-400 font-semibold"
                    : "text-rose-500 font-semibold"
                }
              >
                {download.accept_ranges ? "Yes (Accelerated)" : "No (Single stream)"}
              </span>
            </div>
            <div className="col-span-2 truncate">
              <span className={isClassic ? "text-[#555348] font-semibold" : "text-slate-400"}>
                URL:
              </span>{" "}
              <span className="font-mono text-[10px] text-slate-600 dark:text-slate-300" title={download.url}>
                {download.url}
              </span>
            </div>
            <div className="col-span-2 truncate">
              <span className={isClassic ? "text-[#555348] font-semibold" : "text-slate-400"}>
                Save to:
              </span>{" "}
              <span className="font-mono text-[10px] text-slate-600 dark:text-slate-300" title={download.dir}>
                {download.dir}/{download.filename}
              </span>
            </div>
          </div>

          {/* Primary Overall Progress Bar */}
          <div>
            <div className="mb-1 flex justify-between text-[11px]">
              <span className={isClassic ? "font-bold text-[#333]" : "font-medium text-slate-300"}>
                Overall Progress
              </span>
              <span className="font-bold">{percent}%</span>
            </div>
            <div
              className={
                isClassic
                  ? "h-4 w-full overflow-hidden border border-[#7a786c] bg-[#e3e0d3] shadow-inner"
                  : "h-3 w-full overflow-hidden rounded-full bg-ink-950"
              }
            >
              <div
                className={`h-full transition-all duration-300 ${
                  active
                    ? "bg-gradient-to-r from-emerald-500 via-teal-400 to-emerald-300"
                    : download.state === "done"
                    ? "bg-emerald-500"
                    : "bg-amber-500"
                }`}
                style={{ width: `${Math.max(percent, 2)}%` }}
              />
            </div>
          </div>

          {/* IDM Dynamic Segmentation Visualizer */}
          <div
            className={
              isClassic
                ? "border border-[#b8b5a7] bg-[#fdfcf9] p-3 rounded"
                : "rounded-xl border border-white/5 bg-ink-950/70 p-4"
            }
          >
            <div className="mb-2 flex items-center justify-between">
              <div className="flex items-center gap-1.5 font-semibold text-xs">
                <PulseIcon className="h-4 w-4 text-emerald-500 animate-pulse" />
                <span>Dynamic Segmentation & Connections ({ranges.length || (download.connections || 1)} threads)</span>
              </div>
              <span
                className={
                  isClassic
                    ? "rounded bg-[#0a3d91]/10 px-2 py-0.5 text-[10px] font-bold text-[#0a3d91]"
                    : "rounded bg-accent-500/10 px-2 py-0.5 text-[10px] font-semibold text-accent-300"
                }
              >
                10x Dynamic Rebalancing
              </span>
            </div>

            {/* Segment visual lane */}
            <div
              className={
                isClassic
                  ? "relative h-6 w-full overflow-hidden border border-[#8f8c7f] bg-[#dedbce] flex"
                  : "relative h-5 w-full overflow-hidden rounded-lg bg-ink-900 border border-white/10 flex"
              }
            >
              {ranges.length > 0 && download.total_size > 0 ? (
                ranges.map((range, idx) => {
                  const rangeTotal = range.end - range.start;
                  const rangeWidthPct = (rangeTotal / download.total_size) * 100;
                  const rangeDone = range.pos - range.start;
                  const rangeDonePct = rangeTotal > 0 ? (rangeDone / rangeTotal) * 100 : 0;

                  // Distinct colors for connection segments like IDM
                  const colors = [
                    "bg-emerald-500",
                    "bg-blue-500",
                    "bg-cyan-500",
                    "bg-teal-500",
                    "bg-indigo-500",
                    "bg-violet-500",
                    "bg-amber-500",
                    "bg-lime-500",
                  ];
                  const color = colors[idx % colors.length];

                  return (
                    <div
                      key={idx}
                      className="relative h-full border-r border-black/40 overflow-hidden"
                      style={{ width: `${rangeWidthPct}%` }}
                      title={`Thread #${idx + 1}: ${formatBytes(rangeDone)} of ${formatBytes(rangeTotal)}`}
                    >
                      <div
                        className={`h-full ${color} transition-all duration-200`}
                        style={{ width: `${rangeDonePct}%` }}
                      />
                    </div>
                  );
                })
              ) : (
                <div
                  className="h-full bg-emerald-500 transition-all duration-300"
                  style={{ width: `${percent}%` }}
                />
              )}
            </div>

            {/* Connection list mini-table */}
            <div className="mt-2.5 max-h-32 overflow-y-auto font-mono text-[10px]">
              <div
                className={
                  isClassic
                    ? "grid grid-cols-[60px_1fr_100px] border-b border-[#c8c5b7] pb-1 font-bold text-[#555348]"
                    : "grid grid-cols-[60px_1fr_100px] border-b border-white/10 pb-1 font-semibold text-slate-400"
                }
              >
                <span>Thread</span>
                <span>Byte Range</span>
                <span className="text-right">Downloaded</span>
              </div>
              {ranges.length > 0 ? (
                ranges.map((range, idx) => {
                  const rangeTotal = range.end - range.start;
                  const rangeDone = range.pos - range.start;
                  const isFinished = range.pos >= range.end;
                  return (
                    <div
                      key={idx}
                      className={
                        isClassic
                          ? "grid grid-cols-[60px_1fr_100px] py-0.5 border-b border-[#e9e6da]"
                          : "grid grid-cols-[60px_1fr_100px] py-1 border-b border-white/[0.04] text-slate-300"
                      }
                    >
                      <span className="font-bold text-emerald-600 dark:text-emerald-400">
                        #{idx + 1}
                      </span>
                      <span className="truncate">
                        {range.start.toLocaleString()} – {range.end.toLocaleString()}
                      </span>
                      <span className="text-right tabular-nums">
                        {isFinished ? "100% (Done)" : `${formatBytes(rangeDone)} / ${formatBytes(rangeTotal)}`}
                      </span>
                    </div>
                  );
                })
              ) : (
                <div className="py-2 text-center text-slate-500">
                  {download.accept_ranges
                    ? "Establishing parallel connection workers…"
                    : "Single-connection mode (server does not support Range header)"}
                </div>
              )}
            </div>
          </div>

          {/* Speed Limiter toggle inside progress dialog */}
          <div
            className={
              isClassic
                ? "flex items-center justify-between border border-[#b8b5a7] bg-[#f8f7f2] px-3 py-2 rounded text-[11px]"
                : "flex items-center justify-between rounded-xl border border-white/5 bg-ink-950/60 px-4 py-2.5 text-xs"
            }
          >
            <label className="flex items-center gap-2 cursor-pointer">
              <input
                type="checkbox"
                checked={speedLimitEnabled}
                onChange={(e) => setSpeedLimitEnabled(e.target.checked)}
                className="rounded border-slate-600"
              />
              <SpeedometerIcon className="h-4 w-4 text-emerald-500" />
              <span>Speed Limiter</span>
            </label>
            {speedLimitEnabled ? (
              <div className="flex items-center gap-1.5">
                <input
                  type="number"
                  min="0"
                  step="50"
                  value={speedLimit || 500}
                  onChange={(e) => setSpeedLimit(Number(e.target.value))}
                  className={
                    isClassic
                      ? "h-5 w-20 border border-[#7f9db9] bg-white px-1 text-right text-[11px]"
                      : "h-7 w-24 rounded border border-white/10 bg-ink-900 px-2 text-right text-xs"
                  }
                />
                <span className="text-[10px] text-slate-500">KB/sec</span>
              </div>
            ) : (
              <span className="text-[10px] text-slate-500">Unlimited speed (Maximum wire throughput)</span>
            )}
          </div>
        </div>

        {/* Footer actions */}
        <div
          className={
            isClassic
              ? "mt-4 flex items-center justify-between border-t border-[#b8b5a7] bg-[#e4e1d5] p-2.5 rounded-b"
              : "mt-6 flex items-center justify-between border-t border-white/10 pt-4"
          }
        >
          <div className="flex gap-2">
            <button
              onClick={openFolder}
              className={
                isClassic
                  ? "flex items-center gap-1 border border-[#7f7c6e] bg-gradient-to-b from-[#fdfcf8] to-[#dedbcf] px-2.5 py-1 text-[11px] font-medium hover:bg-[#fff]"
                  : "btn btn-ghost h-8 px-2.5 text-xs"
              }
            >
              <FolderIcon className="h-3.5 w-3.5 text-[#315f9a] dark:text-accent-400" />
              <span>Open Folder</span>
            </button>
            {download.state === "done" && (
              <button
                onClick={openFile}
                className={
                  isClassic
                    ? "flex items-center gap-1 border border-[#7f7c6e] bg-gradient-to-b from-[#fdfcf8] to-[#dedbcf] px-2.5 py-1 text-[11px] font-medium hover:bg-[#fff]"
                    : "btn btn-ghost h-8 px-2.5 text-xs"
                }
              >
                <span>Open File</span>
              </button>
            )}
          </div>

          <div className="flex gap-2">
            {active ? (
              <button
                onClick={() => void pause(download.id)}
                className={
                  isClassic
                    ? "flex items-center gap-1 border border-[#7f7c6e] bg-gradient-to-b from-[#fdfcf8] to-[#dedbcf] px-3 py-1 text-[11px] font-bold hover:bg-[#fff]"
                    : "btn h-8 px-3 text-xs bg-amber-500/20 text-amber-300 border-amber-500/30"
                }
              >
                <PauseIcon className="h-3.5 w-3.5 text-amber-600" />
                <span>Pause</span>
              </button>
            ) : download.state === "paused" ||
              download.state === "error" ||
              download.state === "canceled" ? (
              <button
                onClick={() => void resume(download.id)}
                className={
                  isClassic
                    ? "flex items-center gap-1 border border-[#7f7c6e] bg-gradient-to-b from-[#fdfcf8] to-[#dedbcf] px-3 py-1 text-[11px] font-bold hover:bg-[#fff]"
                    : "btn btn-primary h-8 px-3 text-xs"
                }
              >
                <PlayIcon className="h-3.5 w-3.5 text-emerald-600" />
                <span>Resume</span>
              </button>
            ) : null}

            {active && (
              <button
                onClick={() => void cancel(download.id)}
                className={
                  isClassic
                    ? "flex items-center gap-1 border border-[#7f7c6e] bg-gradient-to-b from-[#fdfcf8] to-[#dedbcf] px-3 py-1 text-[11px] hover:bg-[#fff]"
                    : "btn btn-danger h-8 px-3 text-xs"
                }
              >
                <XIcon className="h-3.5 w-3.5 text-rose-500" />
                <span>Cancel</span>
              </button>
            )}

            <button
              onClick={closeProgressDialog}
              className={
                isClassic
                  ? "border border-[#7f7c6e] bg-gradient-to-b from-[#fdfcf8] to-[#dedbcf] px-3 py-1 text-[11px] hover:bg-[#fff]"
                  : "btn btn-ghost h-8 px-3 text-xs"
              }
            >
              Hide / Close
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
