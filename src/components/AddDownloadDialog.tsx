import { useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useStore } from "../store";
import { api } from "../lib/api";
import type { ProbeResult } from "../lib/types";
import { categoryOfFilename, formatBytes } from "../lib/format";
import { Modal } from "./Modal";
import { DownloadIcon, FolderIcon, PauseIcon, ZapIcon } from "./Icons";

const CATEGORIES = ["auto", "Video", "Audio", "Documents", "Archives", "Programs", "Images", "Other"];

export function AddDownloadDialog() {
  const openState = useStore((state) => state.addOpen);
  const addUrl = useStore((state) => state.addUrl);
  const closeAdd = useStore((state) => state.closeAdd);
  const settings = useStore((state) => state.settings);
  const notify = useStore((state) => state.notify);
  const refresh = useStore((state) => state.refresh);

  const [url, setUrl] = useState("");
  const [filename, setFilename] = useState("");
  const [dir, setDir] = useState("");
  const [category, setCategory] = useState("auto");
  const [connections, setConnections] = useState(8);
  const [limitMbps, setLimitMbps] = useState(0);
  const [startNow, setStartNow] = useState(true);
  const [probe, setProbe] = useState<ProbeResult | null>(null);
  const [probing, setProbing] = useState(false);
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    if (!openState) return;
    setUrl(addUrl);
    setFilename("");
    setProbe(null);
    setDir(settings?.download_dir ?? "");
    setConnections(settings?.connections_per_download ?? 8);
    setLimitMbps(0);
    setCategory("auto");
    setStartNow(true);
  }, [openState, addUrl, settings]);

  const validUrl = useMemo(() => {
    try {
      const parsed = new URL(url);
      return parsed.protocol === "http:" || parsed.protocol === "https:";
    } catch {
      return false;
    }
  }, [url]);

  const analyze = async () => {
    if (!validUrl) {
      notify("Enter a valid http(s) URL", "error");
      return;
    }
    setProbing(true);
    try {
      const result = await api.probe(url);
      setProbe(result);
      if (!filename) setFilename(result.filename);
      if (category === "auto") {
        setCategory(
          CATEGORIES.find(
            (item) => item.toLowerCase() === categoryOfFilename(result.filename),
          ) ?? "auto",
        );
      }
    } catch (error) {
      notify(String(error), "error");
    } finally {
      setProbing(false);
    }
  };

  const pickFolder = async () => {
    const selected = await open({ directory: true, defaultPath: dir || undefined });
    if (typeof selected === "string") setDir(selected);
  };

  const submit = async () => {
    if (!validUrl) {
      notify("Enter a valid http(s) URL", "error");
      return;
    }
    setSubmitting(true);
    try {
      await api.add({
        url,
        dir: dir || null,
        filename: filename || null,
        category: category === "auto" ? null : category,
        connections,
        speed_limit: Math.round(limitMbps * 1024 * 1024),
        priority: 0,
        headers: {},
        start_now: startNow,
      });
      notify(startNow ? "Download started" : "Download queued", "success");
      closeAdd();
      await refresh();
    } catch (error) {
      notify(String(error), "error");
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Modal
      open={openState}
      onClose={closeAdd}
      title="New download"
      subtitle="HyperFetch splits the transfer across connections when the server allows it."
      width="max-w-2xl"
      footer={
        <>
          <button className="btn" onClick={closeAdd} disabled={submitting}>
            Cancel
          </button>
          <button
            className="btn btn-primary min-w-32"
            onClick={() => void submit()}
            disabled={submitting || !validUrl}
          >
            {startNow ? <DownloadIcon className="h-4 w-4" /> : <PauseIcon className="h-4 w-4" />}
            {submitting ? "Adding…" : startNow ? "Start download" : "Add to queue"}
          </button>
        </>
      }
    >
      <div className="space-y-4">
        <div>
          <label className="label" htmlFor="download-url">
            URL
          </label>
          <div className="flex gap-2">
            <div className="relative flex-1">
              <ZapIcon className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-slate-500" />
              <input
                id="download-url"
                className="input pl-9 font-mono text-[12px]"
                placeholder="https://example.com/file.zip"
                value={url}
                onChange={(event) => setUrl(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter" && !probing) void analyze();
                }}
                autoFocus
              />
            </div>
            <button className="btn shrink-0" onClick={() => void analyze()} disabled={probing || !validUrl}>
              {probing ? "Analyzing…" : "Analyze"}
            </button>
          </div>
        </div>

        {probe ? (
          <div className="grid grid-cols-2 gap-3 rounded-xl border border-accent-500/20 bg-accent-500/[0.04] px-4 py-3 text-xs sm:grid-cols-4">
            <div>
              <p className="text-[10px] uppercase tracking-wide text-slate-500">Filename</p>
              <p className="mt-0.5 truncate font-medium text-slate-200" title={probe.filename}>
                {probe.filename}
              </p>
            </div>
            <div>
              <p className="text-[10px] uppercase tracking-wide text-slate-500">Size</p>
              <p className="mt-0.5 font-mono text-slate-200">
                {probe.total_size ? formatBytes(probe.total_size) : "unknown"}
              </p>
            </div>
            <div>
              <p className="text-[10px] uppercase tracking-wide text-slate-500">Resumable</p>
              <p className={`mt-0.5 font-medium ${probe.accept_ranges ? "text-accent-300" : "text-amber-300"}`}>
                {probe.accept_ranges ? "Yes · ranges" : "No · single stream"}
              </p>
            </div>
            <div>
              <p className="text-[10px] uppercase tracking-wide text-slate-500">Type</p>
              <p className="mt-0.5 truncate font-mono text-slate-300">
                {probe.content_type ?? "unknown"}
              </p>
            </div>
          </div>
        ) : null}

        <div className="grid gap-3 sm:grid-cols-2">
          <div>
            <label className="label" htmlFor="download-name">
              Save as
            </label>
            <input
              id="download-name"
              className="input"
              placeholder="Detected automatically"
              value={filename}
              onChange={(event) => setFilename(event.target.value)}
            />
          </div>
          <div>
            <label className="label">Category</label>
            <select
              className="input"
              value={category}
              onChange={(event) => setCategory(event.target.value)}
            >
              {CATEGORIES.map((item) => (
                <option key={item} value={item}>
                  {item === "auto" ? "Auto-detect" : item}
                </option>
              ))}
            </select>
          </div>
        </div>

        <div>
          <label className="label" htmlFor="download-dir">
            Save to
          </label>
          <div className="flex gap-2">
            <div className="relative flex-1">
              <FolderIcon className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-slate-500" />
              <input
                id="download-dir"
                className="input pl-9 font-mono text-[12px]"
                value={dir}
                onChange={(event) => setDir(event.target.value)}
              />
            </div>
            <button className="btn shrink-0" onClick={() => void pickFolder()}>
              Browse
            </button>
          </div>
        </div>

        <div className="grid gap-3 sm:grid-cols-2">
          <div>
            <label className="label" htmlFor="download-connections">
              Connections
            </label>
            <input
              id="download-connections"
              type="number"
              min={1}
              max={settings?.max_connections ?? 32}
              className="input"
              value={connections}
              onChange={(event) =>
                setConnections(Math.max(1, Number(event.target.value) || 1))
              }
            />
            <p className="mt-1 text-[10px] text-slate-500">
              More connections can accelerate servers that support byte ranges.
            </p>
          </div>
          <div>
            <label className="label" htmlFor="download-limit">
              Speed limit (MB/s)
            </label>
            <input
              id="download-limit"
              type="number"
              min={0}
              step={0.5}
              className="input"
              value={limitMbps}
              onChange={(event) => setLimitMbps(Math.max(0, Number(event.target.value) || 0))}
            />
            <p className="mt-1 text-[10px] text-slate-500">0 means unlimited.</p>
          </div>
        </div>

        <label className="flex cursor-pointer items-center gap-2 text-xs text-slate-300">
          <input
            type="checkbox"
            className="h-4 w-4 rounded border-white/20 bg-ink-800 accent-accent-500"
            checked={startNow}
            onChange={(event) => setStartNow(event.target.checked)}
          />
          Start immediately
        </label>
      </div>
    </Modal>
  );
}
