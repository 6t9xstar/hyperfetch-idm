import { useState } from "react";
import { useStore } from "../store";
import { api } from "../lib/api";
import type { DiscoveredLink } from "../lib/types";
import { formatBytes, domainOf } from "../lib/format";
import { Modal } from "./Modal";
import {
  CheckIcon,
  DownloadIcon,
  FilmIcon,
  FolderIcon,
  GlobeIcon,
  LinkIcon,
  ZapIcon,
} from "./Icons";

type Tab = "video" | "site";

export function GrabberDialog() {
  const open = useStore((state) => state.grabberOpen);
  const setOpen = useStore((state) => state.setGrabberOpen);
  const startHls = async (url: string, filename: string | null) => {
    const id = await api.startHls(url, filename);
    return id;
  };
  const trackGrabber = useStore((state) => state.trackGrabber);
  const notify = useStore((state) => state.notify);
  const refresh = useStore((state) => state.refresh);
  const tasks = useStore((state) => state.grabberTasks);
  const clearFinished = useStore((state) => state.clearFinishedGrabbers);

  const [tab, setTab] = useState<Tab>("video");

  const [hlsUrl, setHlsUrl] = useState("");
  const [hlsName, setHlsName] = useState("");
  const [starting, setStarting] = useState(false);

  const [siteUrl, setSiteUrl] = useState("");
  const [depth, setDepth] = useState(2);
  const [maxPages, setMaxPages] = useState(200);
  const [filesOnly, setFilesOnly] = useState(true);
  const [scanning, setScanning] = useState(false);
  const [links, setLinks] = useState<DiscoveredLink[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [adding, setAdding] = useState(false);

  const beginHls = async () => {
    if (!hlsUrl.trim()) return;
    setStarting(true);
    try {
      const name = hlsName.trim() || null;
      const id = await startHls(hlsUrl.trim(), name);
      trackGrabber(id, name ?? domainOf(hlsUrl));
      notify("HLS download started", "success");
      setHlsUrl("");
      setHlsName("");
    } catch (error) {
      notify(String(error), "error");
    } finally {
      setStarting(false);
    }
  };

  const scan = async () => {
    if (!siteUrl.trim()) return;
    setScanning(true);
    try {
      const result = await api.crawl(siteUrl.trim(), depth, maxPages);
      setLinks(result);
      setSelected(new Set(result.filter((link) => !filesOnly || link.is_file).map((link) => link.url)));
      if (result.length === 0) {
        notify("No links found on that page", "info");
      }
    } catch (error) {
      notify(String(error), "error");
    } finally {
      setScanning(false);
    }
  };

  const toggleLink = (url: string) => {
    setSelected((current) => {
      const next = new Set(current);
      if (next.has(url)) next.delete(url);
      else next.add(url);
      return next;
    });
  };

  const toggleAll = (checked: boolean) => {
    setSelected(
      checked
        ? new Set(links.filter((link) => !filesOnly || link.is_file).map((link) => link.url))
        : new Set(),
    );
  };

  const addSelected = async () => {
    if (selected.size === 0) return;
    setAdding(true);
    let added = 0;
    const referer = siteUrl.trim();
    for (const url of selected) {
      try {
        await api.add({
          url,
          dir: null,
          filename: null,
          category: null,
          connections: null,
          speed_limit: 0,
          priority: 0,
          headers: referer ? { Referer: referer } : {},
          start_now: true,
        });
        added += 1;
      } catch {
        /* skip failures, continue with the rest */
      }
    }
    setAdding(false);
    notify(`Added ${added} of ${selected.size} downloads`, added > 0 ? "success" : "error");
    await refresh();
    setSelected(new Set());
  };

  const visibleLinks = filesOnly ? links.filter((link) => link.is_file) : links;

  return (
    <Modal
      open={open}
      onClose={() => setOpen(false)}
      title="Grabber"
      subtitle="Download HLS video streams or collect files from a page."
      width="max-w-3xl"
      footer={
        <button className="btn" onClick={() => setOpen(false)}>
          Close
        </button>
      }
    >
      <div className="mb-4 flex gap-1 rounded-lg border border-white/[0.06] bg-ink-950/60 p-1">
        <button
          className={`flex flex-1 items-center justify-center gap-2 rounded-md px-3 py-1.5 text-xs font-medium transition ${
            tab === "video"
              ? "bg-white/[0.07] text-slate-100"
              : "text-slate-400 hover:text-slate-200"
          }`}
          onClick={() => setTab("video")}
        >
          <FilmIcon className="h-4 w-4 text-accent-400" />
          Video (HLS)
        </button>
        <button
          className={`flex flex-1 items-center justify-center gap-2 rounded-md px-3 py-1.5 text-xs font-medium transition ${
            tab === "site"
              ? "bg-white/[0.07] text-slate-100"
              : "text-slate-400 hover:text-slate-200"
          }`}
          onClick={() => setTab("site")}
        >
          <GlobeIcon className="h-4 w-4 text-sky-400" />
          Site grabber
        </button>
      </div>

      {tab === "video" ? (
        <div className="space-y-4">
          <div>
            <label className="label" htmlFor="hls-url">
              HLS playlist URL (.m3u8)
            </label>
            <div className="flex gap-2">
              <div className="relative flex-1">
                <LinkIcon className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-slate-500" />
                <input
                  id="hls-url"
                  className="input pl-9 font-mono text-[12px]"
                  placeholder="https://example.com/master.m3u8"
                  value={hlsUrl}
                  onChange={(event) => setHlsUrl(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter" && !starting) void beginHls();
                  }}
                />
              </div>
              <button
                className="btn btn-primary shrink-0"
                onClick={() => void beginHls()}
                disabled={starting || !hlsUrl.trim()}
              >
                <DownloadIcon className="h-4 w-4" />
                {starting ? "Starting…" : "Grab video"}
              </button>
            </div>
          </div>
          <div>
            <label className="label" htmlFor="hls-name">
              Output filename (optional)
            </label>
            <input
              id="hls-name"
              className="input"
              placeholder="Video title.ts"
              value={hlsName}
              onChange={(event) => setHlsName(event.target.value)}
            />
          </div>
          <div className="rounded-lg border border-white/[0.05] bg-white/[0.02] px-3 py-2.5 text-[11px] leading-relaxed text-slate-500">
            <span className="font-medium text-slate-300">Good to know:</span> master playlists pick
            the highest-bandwidth variant. Standard AES-128 encryption from the playlist is
            supported. If FFmpeg is on PATH, plain TS output is remuxed to MP4 without re-encoding.
            DRM-protected streams are not supported.
          </div>
        </div>
      ) : (
        <div className="space-y-4">
          <div>
            <label className="label" htmlFor="site-url">
              Page to scan
            </label>
            <div className="flex gap-2">
              <div className="relative flex-1">
                <GlobeIcon className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-slate-500" />
                <input
                  id="site-url"
                  className="input pl-9 font-mono text-[12px]"
                  placeholder="https://example.com/downloads"
                  value={siteUrl}
                  onChange={(event) => setSiteUrl(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter" && !scanning) void scan();
                  }}
                />
              </div>
              <button
                className="btn shrink-0"
                onClick={() => void scan()}
                disabled={scanning || !siteUrl.trim()}
              >
                <ZapIcon className="h-4 w-4 text-accent-400" />
                {scanning ? "Scanning…" : "Scan page"}
              </button>
            </div>
          </div>

          <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
            <div>
              <label className="label" htmlFor="site-depth">
                Depth
              </label>
              <input
                id="site-depth"
                type="number"
                min={0}
                max={6}
                className="input"
                value={depth}
                onChange={(event) => setDepth(Math.max(0, Number(event.target.value) || 0))}
              />
            </div>
            <div>
              <label className="label" htmlFor="site-pages">
                Max pages
              </label>
              <input
                id="site-pages"
                type="number"
                min={1}
                max={2000}
                className="input"
                value={maxPages}
                onChange={(event) => setMaxPages(Math.max(1, Number(event.target.value) || 1))}
              />
            </div>
            <label className="col-span-2 flex cursor-pointer items-center gap-2 self-end rounded-lg border border-white/[0.05] bg-white/[0.02] px-3 py-2 text-xs text-slate-300">
              <input
                type="checkbox"
                className="h-4 w-4 rounded border-white/20 bg-ink-800 accent-accent-500"
                checked={filesOnly}
                onChange={(event) => {
                  setFilesOnly(event.target.checked);
                  setSelected(
                    new Set(
                      links
                        .filter((link) => !event.target.checked || link.is_file)
                        .map((link) => link.url),
                    ),
                  );
                }}
              />
              Only likely files
            </label>
          </div>

          {links.length > 0 ? (
            <div className="overflow-hidden rounded-xl border border-white/[0.06]">
              <div className="flex items-center justify-between border-b border-white/[0.06] bg-white/[0.02] px-3 py-2 text-[11px] text-slate-400">
                <label className="flex cursor-pointer items-center gap-2">
                  <input
                    type="checkbox"
                    className="h-3.5 w-3.5 rounded border-white/20 bg-ink-800 accent-accent-500"
                    checked={visibleLinks.length > 0 && visibleLinks.every((link) => selected.has(link.url))}
                    onChange={(event) => toggleAll(event.target.checked)}
                  />
                  Select all ({visibleLinks.length})
                </label>
                <button
                  className="btn btn-primary h-7 px-2.5 text-[11px]"
                  onClick={() => void addSelected()}
                  disabled={adding || selected.size === 0}
                >
                  <DownloadIcon className="h-3.5 w-3.5" />
                  {adding ? "Adding…" : `Download selected (${selected.size})`}
                </button>
              </div>
              <div className="max-h-56 overflow-y-auto">
                {visibleLinks.map((link) => (
                  <label
                    key={link.url}
                    className="flex cursor-pointer items-center gap-2.5 border-b border-white/[0.03] px-3 py-2 text-[11px] last:border-0 hover:bg-white/[0.02]"
                  >
                    <input
                      type="checkbox"
                      className="h-3.5 w-3.5 shrink-0 rounded border-white/20 bg-ink-800 accent-accent-500"
                      checked={selected.has(link.url)}
                      onChange={() => toggleLink(link.url)}
                    />
                    <span
                      className={`chip shrink-0 ${
                        link.is_file
                          ? "border-accent-500/25 bg-accent-500/10 text-accent-300"
                          : "border-white/10 bg-white/[0.04] text-slate-400"
                      }`}
                    >
                      {link.is_file ? "FILE" : "PAGE"}
                    </span>
                    <span className="min-w-0 flex-1 truncate font-mono text-slate-300" title={link.url}>
                      {link.url}
                    </span>
                    <span className="shrink-0 text-[10px] text-slate-600">d{link.depth}</span>
                  </label>
                ))}
              </div>
            </div>
          ) : (
            <div className="rounded-xl border border-dashed border-white/[0.08] px-4 py-8 text-center text-xs text-slate-500">
              Scan a page to list linked files. The crawler stays on the same host and respects the
              depth limit.
            </div>
          )}
        </div>
      )}

      {tasks.length > 0 ? (
        <div className="mt-5 border-t border-white/[0.06] pt-4">
          <div className="mb-2 flex items-center justify-between">
            <p className="section-title">Grabber activity</p>
            <button className="text-[10px] text-slate-500 hover:text-slate-300" onClick={clearFinished}>
              Clear finished
            </button>
          </div>
          <div className="space-y-2">
            {tasks.map((task) => {
              const percent = task.total > 0 ? Math.min(100, (task.done / task.total) * 100) : 0;
              return (
                <div
                  key={task.id}
                  className="rounded-lg border border-white/[0.05] bg-white/[0.02] px-3 py-2"
                >
                  <div className="flex items-center gap-2 text-[11px]">
                    {task.status === "running" ? (
                      <ZapIcon className="h-3.5 w-3.5 animate-pulse text-accent-400" />
                    ) : task.status === "done" ? (
                      <CheckIcon className="h-3.5 w-3.5 text-accent-400" />
                    ) : (
                      <span className="text-rose-400">!</span>
                    )}
                    <span className="min-w-0 flex-1 truncate text-slate-300">{task.label}</span>
                    <span className="shrink-0 font-mono text-[10px] text-slate-500">
                      {task.status === "running"
                        ? task.total > 0
                          ? `${task.done}/${task.total} · ${formatBytes(task.bytes)}`
                          : "Preparing…"
                        : task.status === "done"
                          ? formatBytes(task.bytes)
                          : "Failed"}
                    </span>
                    {task.status === "done" && task.path ? (
                      <button
                        className="icon-btn h-6 w-6"
                        title="Show in folder"
                        onClick={() => {
                          const dir = task.path!.replace(/[\\/][^\\/]+$/, "");
                          void api.openFolder(dir).catch((error) =>
                            notify(String(error), "error"),
                          );
                        }}
                      >
                        <FolderIcon className="h-3.5 w-3.5" />
                      </button>
                    ) : null}
                  </div>
                  {task.status === "running" ? (
                    <div className="mt-1.5 h-1 overflow-hidden rounded-full bg-white/[0.06]">
                      <div
                        className="h-full rounded-full bg-gradient-to-r from-accent-500 to-accent-300 transition-[width] duration-300"
                        style={{ width: `${percent}%` }}
                      />
                    </div>
                  ) : null}
                  {task.message ? (
                    <p className="mt-1 truncate text-[10px] text-rose-400/80" title={task.message}>
                      {task.message}
                    </p>
                  ) : null}
                </div>
              );
            })}
          </div>
        </div>
      ) : null}
    </Modal>
  );
}
