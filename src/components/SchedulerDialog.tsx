import { useState } from "react";
import { useStore } from "../store";
import { ClockIcon } from "./Icons";
import { formatBytes } from "../lib/format";

export function SchedulerDialog() {
  const schedulerOpen = useStore((state) => state.schedulerOpen);
  const setSchedulerOpen = useStore((state) => state.setSchedulerOpen);
  const downloads = useStore((state) => state.downloads);
  const resumeAll = useStore((state) => state.resumeAll);
  const pauseAll = useStore((state) => state.pauseAll);
  const notify = useStore((state) => state.notify);
  const theme = useStore((state) => state.settings?.theme ?? "modern");
  const isClassic = theme === "classic";

  const [activeTab, setActiveTab] = useState<"schedule" | "queue">("schedule");
  const [startTime, setStartTime] = useState("02:00");
  const [stopTime, setStopTime] = useState("06:00");
  const [startEnabled, setStartEnabled] = useState(false);
  const [stopEnabled, setStopEnabled] = useState(false);
  const [shutdownOnFinish, setShutdownOnFinish] = useState(false);
  const [closeAppOnFinish, setCloseAppOnFinish] = useState(false);
  const [maxRetries, setMaxRetries] = useState(5);

  if (!schedulerOpen) return null;

  const queuedDownloads = downloads.filter((d) => d.state === "queued" || d.state === "paused");

  const handleApply = () => {
    notify("Scheduler settings applied", "success");
    setSchedulerOpen(false);
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4 backdrop-blur-sm"
      onClick={() => setSchedulerOpen(false)}
    >
      <div
        className={
          isClassic
            ? "w-full max-w-lg rounded border border-[#7a786c] bg-[#efede3] text-[#1a1a1a] shadow-2xl font-sans text-xs"
            : "w-full max-w-lg rounded-2xl border border-white/10 bg-ink-900 p-6 text-slate-100 shadow-2xl"
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
          <div className="flex items-center gap-2">
            <ClockIcon className="h-4 w-4 text-emerald-400" />
            <span>IDM Download Scheduler & Queue</span>
          </div>
          <button
            onClick={() => setSchedulerOpen(false)}
            className={
              isClassic
                ? "flex h-4 w-4 items-center justify-center rounded bg-[#c53929] hover:bg-rose-600 text-white text-[10px] leading-none"
                : "text-slate-400 hover:text-white"
            }
          >
            ✕
          </button>
        </div>

        {/* Tab Header */}
        <div className={isClassic ? "flex border-b border-[#9c9a8e] bg-[#e4e1d5] px-3 pt-2 gap-1" : "flex border-b border-white/10 px-4 pt-4 gap-2"}>
          <button
            onClick={() => setActiveTab("schedule")}
            className={
              isClassic
                ? `px-3 py-1 border border-b-0 rounded-t ${
                    activeTab === "schedule"
                      ? "bg-[#efede3] border-[#7a786c] font-bold text-[#0a3d91]"
                      : "bg-[#dfdcd0] border-[#a8a598] text-[#555]"
                  }`
                : `pb-2 px-3 text-xs font-semibold border-b-2 transition ${
                    activeTab === "schedule"
                      ? "border-accent-400 text-accent-300"
                      : "border-transparent text-slate-400 hover:text-slate-200"
                  }`
            }
          >
            Schedule
          </button>
          <button
            onClick={() => setActiveTab("queue")}
            className={
              isClassic
                ? `px-3 py-1 border border-b-0 rounded-t ${
                    activeTab === "queue"
                      ? "bg-[#efede3] border-[#7a786c] font-bold text-[#0a3d91]"
                      : "bg-[#dfdcd0] border-[#a8a598] text-[#555]"
                  }`
                : `pb-2 px-3 text-xs font-semibold border-b-2 transition ${
                    activeTab === "queue"
                      ? "border-accent-400 text-accent-300"
                      : "border-transparent text-slate-400 hover:text-slate-200"
                  }`
            }
          >
            Files in queue ({queuedDownloads.length})
          </button>
        </div>

        {/* Content */}
        <div className={isClassic ? "p-4 space-y-3.5" : "p-4 space-y-4"}>
          {activeTab === "schedule" ? (
            <div className="space-y-3">
              {/* Start schedule */}
              <div
                className={
                  isClassic
                    ? "border border-[#b8b5a7] bg-[#f8f7f2] p-3 rounded space-y-2"
                    : "rounded-xl border border-white/5 bg-ink-950/60 p-4 space-y-3"
                }
              >
                <label className="flex items-center gap-2 font-semibold">
                  <input
                    type="checkbox"
                    checked={startEnabled}
                    onChange={(e) => setStartEnabled(e.target.checked)}
                  />
                  <span>Start download queue at:</span>
                  <input
                    type="time"
                    disabled={!startEnabled}
                    value={startTime}
                    onChange={(e) => setStartTime(e.target.value)}
                    className={
                      isClassic
                        ? "h-5 border border-[#7f9db9] bg-white px-1 text-[11px]"
                        : "h-7 rounded border border-white/10 bg-ink-900 px-2 text-xs"
                    }
                  />
                </label>

                <label className="flex items-center gap-2 font-semibold">
                  <input
                    type="checkbox"
                    checked={stopEnabled}
                    onChange={(e) => setStopEnabled(e.target.checked)}
                  />
                  <span>Stop download queue at:</span>
                  <input
                    type="time"
                    disabled={!stopEnabled}
                    value={stopTime}
                    onChange={(e) => setStopTime(e.target.value)}
                    className={
                      isClassic
                        ? "h-5 border border-[#7f9db9] bg-white px-1 text-[11px]"
                        : "h-7 rounded border border-white/10 bg-ink-900 px-2 text-xs"
                    }
                  />
                </label>
              </div>

              {/* Completion actions */}
              <div
                className={
                  isClassic
                    ? "border border-[#b8b5a7] bg-[#f8f7f2] p-3 rounded space-y-2 text-[11px]"
                    : "rounded-xl border border-white/5 bg-ink-950/60 p-4 space-y-2.5 text-xs"
                }
              >
                <div className="font-bold text-[#444] dark:text-slate-300">
                  When queue finishes:
                </div>
                <label className="flex items-center gap-2">
                  <input
                    type="checkbox"
                    checked={closeAppOnFinish}
                    onChange={(e) => setCloseAppOnFinish(e.target.checked)}
                  />
                  <span>Close HyperFetch when queue finishes</span>
                </label>
                <label className="flex items-center gap-2">
                  <input
                    type="checkbox"
                    checked={shutdownOnFinish}
                    onChange={(e) => setShutdownOnFinish(e.target.checked)}
                  />
                  <span>Turn off computer (shutdown) when queue finishes</span>
                </label>
                <div className="flex items-center gap-2 pt-1">
                  <span>Number of retries for failed files:</span>
                  <input
                    type="number"
                    min="1"
                    max="99"
                    value={maxRetries}
                    onChange={(e) => setMaxRetries(Number(e.target.value))}
                    className={
                      isClassic
                        ? "h-5 w-14 border border-[#7f9db9] bg-white px-1 text-right text-[11px]"
                        : "h-7 w-16 rounded border border-white/10 bg-ink-900 px-2 text-right text-xs"
                    }
                  />
                </div>
              </div>
            </div>
          ) : (
            <div
              className={
                isClassic
                  ? "border border-[#b8b5a7] bg-[#f8f7f2] p-2 rounded max-h-56 overflow-y-auto"
                  : "rounded-xl border border-white/5 bg-ink-950/60 p-3 max-h-56 overflow-y-auto"
              }
            >
              {queuedDownloads.length === 0 ? (
                <div className="py-8 text-center text-slate-500">
                  No files currently in the download queue.
                </div>
              ) : (
                <div className="space-y-1">
                  {queuedDownloads.map((item, idx) => (
                    <div
                      key={item.id}
                      className={
                        isClassic
                          ? "flex items-center justify-between p-1.5 border-b border-[#e5e2d6] text-[11px]"
                          : "flex items-center justify-between p-2 rounded bg-ink-900 border border-white/5 text-xs"
                      }
                    >
                      <div className="flex items-center gap-2 truncate">
                        <span className="font-bold text-slate-400">#{idx + 1}</span>
                        <span className="truncate font-medium">{item.filename}</span>
                      </div>
                      <span className="shrink-0 text-slate-500">
                        {item.total_size > 0 ? formatBytes(item.total_size) : "—"}
                      </span>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}
        </div>

        {/* Footer */}
        <div
          className={
            isClassic
              ? "flex items-center justify-between border-t border-[#b8b5a7] bg-[#e4e1d5] p-2.5 rounded-b"
              : "flex items-center justify-between border-t border-white/10 p-4"
          }
        >
          <div className="flex gap-2">
            <button
              onClick={() => void resumeAll()}
              className={
                isClassic
                  ? "border border-[#7f7c6e] bg-gradient-to-b from-[#fdfcf8] to-[#dedbcf] px-3 py-1 text-[11px] font-bold hover:bg-[#fff]"
                  : "btn btn-primary h-8 px-3 text-xs"
              }
            >
              Start Queue Now
            </button>
            <button
              onClick={() => void pauseAll()}
              className={
                isClassic
                  ? "border border-[#7f7c6e] bg-gradient-to-b from-[#fdfcf8] to-[#dedbcf] px-3 py-1 text-[11px] hover:bg-[#fff]"
                  : "btn btn-ghost h-8 px-3 text-xs"
              }
            >
              Stop Queue
            </button>
          </div>
          <div className="flex gap-2">
            <button
              onClick={handleApply}
              className={
                isClassic
                  ? "border border-[#7f7c6e] bg-gradient-to-b from-[#fdfcf8] to-[#dedbcf] px-4 py-1 text-[11px] font-bold hover:bg-[#fff]"
                  : "btn btn-primary h-8 px-4 text-xs"
              }
            >
              Apply
            </button>
            <button
              onClick={() => setSchedulerOpen(false)}
              className={
                isClassic
                  ? "border border-[#7f7c6e] bg-gradient-to-b from-[#fdfcf8] to-[#dedbcf] px-3 py-1 text-[11px] hover:bg-[#fff]"
                  : "btn btn-ghost h-8 px-3 text-xs"
              }
            >
              Close
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
