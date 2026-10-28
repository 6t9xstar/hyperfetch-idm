import { useState } from "react";
import { useStore } from "../store";
import { api } from "../lib/api";
import { formatBytes, formatPercent, STATE_LABEL } from "../lib/format";
import { CheckIcon, CopyIcon, InfoIcon } from "./Icons";

export function PropertiesDialog() {
  const download = useStore((state) => state.propertiesDownload);
  const closeProperties = useStore((state) => state.closeProperties);
  const notify = useStore((state) => state.notify);
  const theme = useStore((state) => state.settings?.theme ?? "modern");
  const isClassic = theme === "classic";

  const [copied, setCopied] = useState(false);

  if (!download) return null;

  const copyUrl = () => {
    void navigator.clipboard
      .writeText(download.url)
      .then(() => {
        setCopied(true);
        notify("URL copied to clipboard", "success");
        setTimeout(() => setCopied(false), 2000);
      })
      .catch((err) => notify(String(err), "error"));
  };

  const copyHash = () => {
    if (!download.sha256) return;
    void navigator.clipboard
      .writeText(download.sha256)
      .then(() => notify("SHA-256 copied to clipboard", "success"))
      .catch((err) => notify(String(err), "error"));
  };

  const openFolder = () => {
    void api.openFolder(download.dir).catch((err) => notify(String(err), "error"));
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4 backdrop-blur-sm"
      onClick={closeProperties}
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
            <InfoIcon className="h-4 w-4 text-emerald-400" />
            <span>File Properties: {download.filename}</span>
          </div>
          <button
            onClick={closeProperties}
            className={
              isClassic
                ? "flex h-4 w-4 items-center justify-center rounded bg-[#c53929] hover:bg-rose-600 text-white text-[10px] leading-none"
                : "text-slate-400 hover:text-white"
            }
          >
            ✕
          </button>
        </div>

        {/* Content */}
        <div className={isClassic ? "p-4 space-y-3" : "p-4 space-y-4"}>
          {/* General Box */}
          <div
            className={
              isClassic
                ? "border border-[#b8b5a7] bg-[#f8f7f2] p-3 rounded space-y-1.5 text-[11px]"
                : "rounded-xl border border-white/5 bg-ink-950/60 p-4 space-y-2 text-xs"
            }
          >
            <div className="flex justify-between">
              <span className={isClassic ? "text-[#555] font-semibold" : "text-slate-400"}>
                File Name:
              </span>
              <span className="font-bold">{download.filename}</span>
            </div>
            <div className="flex justify-between">
              <span className={isClassic ? "text-[#555] font-semibold" : "text-slate-400"}>
                Status:
              </span>
              <span className="font-semibold">{STATE_LABEL[download.state]}</span>
            </div>
            <div className="flex justify-between">
              <span className={isClassic ? "text-[#555] font-semibold" : "text-slate-400"}>
                Total Size:
              </span>
              <span>{download.total_size > 0 ? formatBytes(download.total_size) : "Unknown"}</span>
            </div>
            <div className="flex justify-between">
              <span className={isClassic ? "text-[#555] font-semibold" : "text-slate-400"}>
                Downloaded:
              </span>
              <span>
                {formatBytes(download.received)} ({formatPercent(download)})
              </span>
            </div>
            <div className="flex justify-between">
              <span className={isClassic ? "text-[#555] font-semibold" : "text-slate-400"}>
                Multi-Connection Acceleration:
              </span>
              <span className="font-semibold text-emerald-600 dark:text-emerald-400">
                {download.accept_ranges
                  ? `Yes (${download.connections} threads)`
                  : "Single Stream"}
              </span>
            </div>
            <div className="flex justify-between">
              <span className={isClassic ? "text-[#555] font-semibold" : "text-slate-400"}>
                Folder:
              </span>
              <span className="truncate max-w-[280px] font-mono text-[10px]" title={download.dir}>
                {download.dir}
              </span>
            </div>
          </div>

          {/* URL & Network Box */}
          <div
            className={
              isClassic
                ? "border border-[#b8b5a7] bg-[#f8f7f2] p-3 rounded space-y-2 text-[11px]"
                : "rounded-xl border border-white/5 bg-ink-950/60 p-4 space-y-2.5 text-xs"
            }
          >
            <div>
              <div className="flex items-center justify-between mb-1">
                <span className={isClassic ? "text-[#555] font-semibold" : "text-slate-400"}>
                  Source URL:
                </span>
                <button
                  onClick={copyUrl}
                  className="flex items-center gap-1 text-[10px] text-blue-600 dark:text-accent-400 hover:underline"
                >
                  {copied ? <CheckIcon className="h-3 w-3" /> : <CopyIcon className="h-3 w-3" />}
                  <span>{copied ? "Copied" : "Copy URL"}</span>
                </button>
              </div>
              <div
                className={
                  isClassic
                    ? "border border-[#7f9db9] bg-white p-1.5 font-mono text-[10px] break-all max-h-16 overflow-y-auto"
                    : "rounded border border-white/10 bg-ink-900 p-2 font-mono text-[10px] break-all max-h-16 overflow-y-auto text-slate-300"
                }
              >
                {download.url}
              </div>
            </div>

            {/* SHA-256 hash if available */}
            {download.sha256 && (
              <div>
                <div className="flex items-center justify-between mb-1">
                  <span className={isClassic ? "text-[#555] font-semibold" : "text-slate-400"}>
                    SHA-256 Checksum:
                  </span>
                  <button
                    onClick={copyHash}
                    className="flex items-center gap-1 text-[10px] text-blue-600 dark:text-accent-400 hover:underline"
                  >
                    <CopyIcon className="h-3 w-3" />
                    <span>Copy</span>
                  </button>
                </div>
                <div
                  className={
                    isClassic
                      ? "border border-[#7f9db9] bg-white p-1.5 font-mono text-[10px] text-emerald-700 break-all select-all"
                      : "rounded border border-white/10 bg-ink-900 p-2 font-mono text-[10px] text-emerald-400 break-all select-all"
                  }
                >
                  {download.sha256}
                </div>
              </div>
            )}
          </div>
        </div>

        {/* Footer */}
        <div
          className={
            isClassic
              ? "flex items-center justify-between border-t border-[#b8b5a7] bg-[#e4e1d5] p-2.5 rounded-b"
              : "flex items-center justify-between border-t border-white/10 p-4"
          }
        >
          <button
            onClick={openFolder}
            className={
              isClassic
                ? "border border-[#7f7c6e] bg-gradient-to-b from-[#fdfcf8] to-[#dedbcf] px-3 py-1 text-[11px] font-medium hover:bg-[#fff]"
                : "btn btn-ghost h-8 px-3 text-xs"
            }
          >
            Open Folder
          </button>
          <button
            onClick={closeProperties}
            className={
              isClassic
                ? "border border-[#7f7c6e] bg-gradient-to-b from-[#fdfcf8] to-[#dedbcf] px-4 py-1 text-[11px] font-bold hover:bg-[#fff]"
                : "btn btn-primary h-8 px-4 text-xs"
            }
          >
            Close
          </button>
        </div>
      </div>
    </div>
  );
}
