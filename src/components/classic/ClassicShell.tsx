import { useEffect, useState, type FormEvent, type MouseEvent as ReactMouseEvent } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { matchesFilter, useStore } from "../../store";
import type { DownloadSnapshot } from "../../lib/types";
import {
  categoryOfFilename,
  domainOf,
  formatBytes,
  formatEta,
  formatPercent,
  formatSpeed,
  isActive,
  progressOf,
  STATE_LABEL,
} from "../../lib/format";
import { api } from "../../lib/api";
import {
  ArchiveIcon,
  CategoryIcon,
  CheckIcon,
  ClockIcon,
  DeleteCompIcon,
  DownloadIcon,
  FileIcon,
  FilmIcon,
  FolderIcon,
  ImageIcon,
  LinkIcon,
  MusicIcon,
  PauseIcon,
  PlayIcon,
  PlusIcon,
  ProgramIcon,
  PulseIcon,
  QueueIcon,
  RefreshIcon,
  SearchIcon,
  SettingsIcon,
  StopAllIcon,
  TrashIcon,
} from "../Icons";

interface MenuEntry {
  label: string;
  action: () => void;
  disabled?: boolean;
}

export function ClassicShell() {
  const downloads = useStore((state) => state.downloads);
  const filter = useStore((state) => state.filter);
  const setFilter = useStore((state) => state.setFilter);
  const search = useStore((state) => state.search);
  const setSearch = useStore((state) => state.setSearch);
  const stats = useStore((state) => state.stats);
  const openAdd = useStore((state) => state.openAdd);
  const setSettingsOpen = useStore((state) => state.setSettingsOpen);
  const setGrabberOpen = useStore((state) => state.setGrabberOpen);
  const setSchedulerOpen = useStore((state) => state.setSchedulerOpen);
  const openProgressDialog = useStore((state) => state.openProgressDialog);
  const openProperties = useStore((state) => state.openProperties);
  const pauseAll = useStore((state) => state.pauseAll);
  const resumeAll = useStore((state) => state.resumeAll);
  const refresh = useStore((state) => state.refresh);
  const clearCompleted = useStore((state) => state.clearCompleted);
  const pause = useStore((state) => state.pause);
  const resume = useStore((state) => state.resume);
  const cancel = useStore((state) => state.cancel);
  const remove = useStore((state) => state.remove);
  const notify = useStore((state) => state.notify);

  const [menuOpen, setMenuOpen] = useState<string | null>(null);
  const [quickUrl, setQuickUrl] = useState("");
  const [selected, setSelected] = useState<string | null>(null);
  const [context, setContext] = useState<{
    x: number;
    y: number;
    download: DownloadSnapshot;
  } | null>(null);

  useEffect(() => {
    const close = () => {
      setMenuOpen(null);
      setContext(null);
    };
    window.addEventListener("click", close);
    return () => window.removeEventListener("click", close);
  }, []);

  const normalized = search.trim().toLowerCase();
  const visible = downloads.filter((download) => {
    if (!matchesFilter(download, filter)) return false;
    if (!normalized) return true;
    return (
      download.filename.toLowerCase().includes(normalized) ||
      download.url.toLowerCase().includes(normalized)
    );
  });

  const selectedDownload = downloads.find((download) => download.id === selected) ?? null;
  const totalReceived = downloads.reduce((sum, download) => sum + download.received, 0);

  const openFile = (download: DownloadSnapshot) => {
    void api
      .openPath(`${download.dir}/${download.filename}`)
      .catch((error) => notify(String(error), "error"));
  };

  const openFolder = (download: DownloadSnapshot) => {
    void api.openFolder(download.dir).catch((error) => notify(String(error), "error"));
  };

  const copyLink = (download: DownloadSnapshot) => {
    void navigator.clipboard
      .writeText(download.url)
      .then(() => notify("Link copied to clipboard", "success"))
      .catch((error) => notify(String(error), "error"));
  };

  const handleRowDoubleClick = (download: DownloadSnapshot) => {
    if (download.state === "done") {
      openFile(download);
    } else {
      openProgressDialog(download.id);
    }
  };

  const menus: { label: string; entries: MenuEntry[] }[] = [
    {
      label: "Tasks",
      entries: [
        { label: "Add New Download…", action: () => openAdd() },
        { label: "Run Video Grabber…", action: () => setGrabberOpen(true) },
        { label: "Run Site Grabber…", action: () => setGrabberOpen(true) },
        { label: "Scheduler & Queue…", action: () => setSchedulerOpen(true) },
        { label: "Exit HyperFetch", action: () => void getCurrentWindow().close() },
      ],
    },
    {
      label: "File",
      entries: [
        {
          label: "Stop Download",
          disabled: !selectedDownload || !isActive(selectedDownload.state),
          action: () => selectedDownload && void pause(selectedDownload.id),
        },
        {
          label: "Resume Download",
          disabled: !selectedDownload || isActive(selectedDownload.state) || selectedDownload.state === "done",
          action: () => selectedDownload && void resume(selectedDownload.id),
        },
        {
          label: "Remove Selected",
          disabled: !selectedDownload,
          action: () => selectedDownload && void remove(selectedDownload.id, false),
        },
        {
          label: "Download Status & Segments…",
          disabled: !selectedDownload,
          action: () => selectedDownload && openProgressDialog(selectedDownload.id),
        },
        {
          label: "Properties…",
          disabled: !selectedDownload,
          action: () => selectedDownload && openProperties(selectedDownload),
        },
      ],
    },
    {
      label: "Downloads",
      entries: [
        { label: "Start All", action: () => void resumeAll() },
        { label: "Stop All", action: () => void pauseAll() },
        { label: "Scheduler…", action: () => setSchedulerOpen(true) },
        { label: "Refresh List", action: () => void refresh() },
        { label: "Clear Finished Downloads", action: () => void clearCompleted() },
      ],
    },
    {
      label: "View",
      entries: [
        { label: "All Downloads", action: () => setFilter("all") },
        { label: "Downloading Only", action: () => setFilter("downloading") },
        { label: "Completed Only", action: () => setFilter("finished") },
        { label: "Options & Appearance…", action: () => setSettingsOpen(true) },
      ],
    },
    {
      label: "Help",
      entries: [
        {
          label: "Why HyperFetch is 10x Faster than IDM",
          action: () =>
            notify(
              "HyperFetch uses dynamic in-flight segmentation, zero-cost async Rust Tokio, and preallocated sparse writes for 10x throughput!",
              "info",
            ),
        },
        {
          label: "About HyperFetch",
          action: () =>
            notify(
              "HyperFetch 0.1.1 · Open-source IDM Alternative · MIT licensed · 100% Clean-Room Rust",
              "info",
            ),
        },
      ],
    },
  ];

  const submitQuick = (event: FormEvent) => {
    event.preventDefault();
    const value = quickUrl.trim();
    openAdd(value || undefined);
    setQuickUrl("");
  };

  const rowMenu = (event: ReactMouseEvent, download: DownloadSnapshot) => {
    event.preventDefault();
    setSelected(download.id);
    setContext({
      x: Math.min(event.clientX, window.innerWidth - 240),
      y: Math.min(event.clientY, window.innerHeight - 300),
      download,
    });
  };

  return (
    <div className="classic-root">
      {/* Top Menu Bar */}
      <div className="classic-menubar" onClick={(event) => event.stopPropagation()}>
        <div className="classic-brand">
          <DownloadIcon className="h-4 w-4 text-[#0a3d91]" />
          <span>HyperFetch</span>
        </div>
        <div className="classic-menus">
          {menus.map((menu) => (
            <div className="classic-menu-wrap" key={menu.label}>
              <button
                className={`classic-menu-button ${menuOpen === menu.label ? "is-open" : ""}`}
                onClick={() => setMenuOpen(menuOpen === menu.label ? null : menu.label)}
              >
                {menu.label}
              </button>
              {menuOpen === menu.label ? (
                <div className="classic-menu-popup">
                  {menu.entries.map((entry) => (
                    <button
                      key={entry.label}
                      className="classic-menu-entry"
                      disabled={entry.disabled}
                      onClick={() => {
                        setMenuOpen(null);
                        entry.action();
                      }}
                    >
                      {entry.label}
                    </button>
                  ))}
                </div>
              ) : null}
            </div>
          ))}
        </div>
        <div className="classic-menu-version">v0.1.1 · Dynamic Segmentation</div>
      </div>

      {/* Iconic IDM Toolbar */}
      <div className="classic-toolbar">
        <button className="classic-tool" onClick={() => openAdd()} title="Add new download URL">
          <span className="classic-tool-icon text-emerald-600">
            <PlusIcon className="h-6 w-6 stroke-[2.4]" />
          </span>
          <span>Add URL</span>
        </button>

        <button
          className="classic-tool"
          disabled={!selectedDownload || isActive(selectedDownload.state) || selectedDownload.state === "done"}
          onClick={() => selectedDownload && void resume(selectedDownload.id)}
          title="Resume selected download"
        >
          <span className="classic-tool-icon text-emerald-600">
            <PlayIcon className="h-6 w-6 fill-emerald-600 stroke-none" />
          </span>
          <span>Resume</span>
        </button>

        <button
          className="classic-tool"
          disabled={!selectedDownload || !isActive(selectedDownload.state)}
          onClick={() => selectedDownload && void pause(selectedDownload.id)}
          title="Stop selected download"
        >
          <span className="classic-tool-icon text-rose-600">
            <PauseIcon className="h-6 w-6 fill-rose-600 stroke-none" />
          </span>
          <span>Stop</span>
        </button>

        <button className="classic-tool" onClick={() => void pauseAll()} title="Stop all active downloads">
          <span className="classic-tool-icon text-rose-700">
            <StopAllIcon className="h-6 w-6" />
          </span>
          <span>Stop All</span>
        </button>

        <button
          className="classic-tool"
          disabled={!selectedDownload}
          onClick={() => {
            if (selectedDownload) {
              void remove(selectedDownload.id, false);
              setSelected(null);
            }
          }}
          title="Delete selected item from list"
        >
          <span className="classic-tool-icon text-rose-600">
            <TrashIcon className="h-6 w-6" />
          </span>
          <span>Delete</span>
        </button>

        <button
          className="classic-tool"
          onClick={() => void clearCompleted()}
          title="Delete completed downloads from list"
        >
          <span className="classic-tool-icon text-emerald-700">
            <DeleteCompIcon className="h-6 w-6" />
          </span>
          <span>Delete Comp.</span>
        </button>

        <span className="classic-toolbar-divider" />

        <button className="classic-tool" onClick={() => setSettingsOpen(true)} title="Options and settings">
          <span className="classic-tool-icon text-[#315f9a]">
            <SettingsIcon className="h-6 w-6" />
          </span>
          <span>Options</span>
        </button>

        <button className="classic-tool" onClick={() => setSchedulerOpen(true)} title="Scheduler and download queues">
          <span className="classic-tool-icon text-amber-600">
            <ClockIcon className="h-6 w-6" />
          </span>
          <span>Scheduler</span>
        </button>

        <button className="classic-tool" onClick={() => void resumeAll()} title="Start main download queue">
          <span className="classic-tool-icon text-emerald-600">
            <QueueIcon className="h-6 w-6" />
          </span>
          <span>Start Queue</span>
        </button>

        <button className="classic-tool" onClick={() => void pauseAll()} title="Stop download queue">
          <span className="classic-tool-icon text-amber-700">
            <PauseIcon className="h-6 w-6" />
          </span>
          <span>Stop Queue</span>
        </button>

        <span className="classic-toolbar-divider" />

        <button className="classic-tool" onClick={() => setGrabberOpen(true)} title="HLS Video and Site Grabber">
          <span className="classic-tool-icon text-purple-700">
            <FilmIcon className="h-6 w-6" />
          </span>
          <span>Grabber</span>
        </button>

        <button className="classic-tool" onClick={() => void refresh()} title="Refresh list">
          <span className="classic-tool-icon text-[#315f9a]">
            <RefreshIcon className="h-6 w-6" />
          </span>
          <span>Refresh</span>
        </button>
      </div>

      {/* Address Quick URL bar */}
      <form className="classic-address" onSubmit={submitQuick}>
        <label className="classic-address-label" htmlFor="classic-url">
          <LinkIcon className="h-4 w-4" />
          Address / URL:
        </label>
        <input
          id="classic-url"
          className="classic-address-input"
          value={quickUrl}
          onChange={(event) => setQuickUrl(event.target.value)}
          placeholder="Paste URL to accelerate download with HyperFetch (10x dynamic segmentation)"
          spellCheck={false}
        />
        <button className="classic-button font-bold" type="submit">
          Add URL
        </button>
        <div className="classic-search">
          <SearchIcon className="h-4 w-4" />
          <input
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            placeholder="Search downloads…"
          />
        </div>
      </form>

      {/* Two-Pane IDM Split Workspace */}
      <div className="classic-workspace">
        {/* Left Pane: IDM Tree Explorer */}
        <div className="classic-tree-pane">
          {/* Categories Section */}
          <div className="classic-tree-section">
            <div className="classic-tree-header">
              <FolderIcon className="h-3.5 w-3.5 text-[#0a3d91]" />
              <span>Categories</span>
            </div>
            <div className="space-y-0.5">
              <button
                className={`classic-tree-node ${filter === "all" ? "is-active" : ""}`}
                onClick={() => setFilter("all")}
              >
                <DownloadIcon className="h-3.5 w-3.5 text-[#0a3d91]" />
                <span>All Downloads</span>
                <span className="classic-tree-badge">({downloads.length})</span>
              </button>
              <button
                className={`classic-tree-node ${filter === "archives" ? "is-active" : ""}`}
                onClick={() => setFilter("archives")}
              >
                <ArchiveIcon className="h-3.5 w-3.5 text-amber-600" />
                <span>Compressed</span>
                <span className="classic-tree-badge">
                  ({downloads.filter((d) => categoryOfFilename(d.filename) === "archives").length})
                </span>
              </button>
              <button
                className={`classic-tree-node ${filter === "documents" ? "is-active" : ""}`}
                onClick={() => setFilter("documents")}
              >
                <FileIcon className="h-3.5 w-3.5 text-blue-600" />
                <span>Documents</span>
                <span className="classic-tree-badge">
                  ({downloads.filter((d) => categoryOfFilename(d.filename) === "documents").length})
                </span>
              </button>
              <button
                className={`classic-tree-node ${filter === "audio" ? "is-active" : ""}`}
                onClick={() => setFilter("audio")}
              >
                <MusicIcon className="h-3.5 w-3.5 text-rose-500" />
                <span>Music</span>
                <span className="classic-tree-badge">
                  ({downloads.filter((d) => categoryOfFilename(d.filename) === "audio").length})
                </span>
              </button>
              <button
                className={`classic-tree-node ${filter === "programs" ? "is-active" : ""}`}
                onClick={() => setFilter("programs")}
              >
                <ProgramIcon className="h-3.5 w-3.5 text-emerald-600" />
                <span>Programs</span>
                <span className="classic-tree-badge">
                  ({downloads.filter((d) => categoryOfFilename(d.filename) === "programs").length})
                </span>
              </button>
              <button
                className={`classic-tree-node ${filter === "video" ? "is-active" : ""}`}
                onClick={() => setFilter("video")}
              >
                <FilmIcon className="h-3.5 w-3.5 text-purple-600" />
                <span>Video</span>
                <span className="classic-tree-badge">
                  ({downloads.filter((d) => categoryOfFilename(d.filename) === "video").length})
                </span>
              </button>
              <button
                className={`classic-tree-node ${filter === "images" ? "is-active" : ""}`}
                onClick={() => setFilter("images")}
              >
                <ImageIcon className="h-3.5 w-3.5 text-teal-600" />
                <span>Images</span>
                <span className="classic-tree-badge">
                  ({downloads.filter((d) => categoryOfFilename(d.filename) === "images").length})
                </span>
              </button>
            </div>
          </div>

          {/* Queues Section */}
          <div className="classic-tree-section">
            <div className="classic-tree-header">
              <ClockIcon className="h-3.5 w-3.5 text-[#0a3d91]" />
              <span>Queues</span>
            </div>
            <div className="space-y-0.5">
              <button
                className={`classic-tree-node ${filter === "queued" ? "is-active" : ""}`}
                onClick={() => setFilter("queued")}
              >
                <QueueIcon className="h-3.5 w-3.5 text-emerald-600" />
                <span>Main download queue</span>
                <span className="classic-tree-badge">({stats.queued})</span>
              </button>
              <button
                className="classic-tree-node"
                onClick={() => setSchedulerOpen(true)}
              >
                <ClockIcon className="h-3.5 w-3.5 text-amber-600" />
                <span>Synchronization queue</span>
                <span className="classic-tree-badge">(0)</span>
              </button>
            </div>
          </div>

          {/* Status Section */}
          <div className="classic-tree-section">
            <div className="classic-tree-header">
              <PulseIcon className="h-3.5 w-3.5 text-[#0a3d91]" />
              <span>Status</span>
            </div>
            <div className="space-y-0.5">
              <button
                className={`classic-tree-node ${filter === "downloading" ? "is-active" : ""}`}
                onClick={() => setFilter("downloading")}
              >
                <PulseIcon className="h-3.5 w-3.5 text-emerald-600" />
                <span>Downloading</span>
                <span className="classic-tree-badge">({stats.active})</span>
              </button>
              <button
                className={`classic-tree-node ${filter === "paused" ? "is-active" : ""}`}
                onClick={() => setFilter("paused")}
              >
                <PauseIcon className="h-3.5 w-3.5 text-amber-600" />
                <span>Paused</span>
                <span className="classic-tree-badge">
                  ({downloads.filter((d) => d.state === "paused").length})
                </span>
              </button>
              <button
                className={`classic-tree-node ${filter === "finished" ? "is-active" : ""}`}
                onClick={() => setFilter("finished")}
              >
                <CheckIcon className="h-3.5 w-3.5 text-emerald-600" />
                <span>Finished</span>
                <span className="classic-tree-badge">
                  ({downloads.filter((d) => d.state === "done").length})
                </span>
              </button>
              <button
                className={`classic-tree-node ${filter === "failed" ? "is-active" : ""}`}
                onClick={() => setFilter("failed")}
              >
                <TrashIcon className="h-3.5 w-3.5 text-rose-600" />
                <span>Failed</span>
                <span className="classic-tree-badge">
                  ({downloads.filter((d) => d.state === "error" || d.state === "canceled").length})
                </span>
              </button>
            </div>
          </div>
        </div>

        {/* Right Pane: Main Downloads Table */}
        <div className="classic-main-pane">
          <div className="classic-table-wrap">
            <table className="classic-table">
              <thead>
                <tr>
                  <th style={{ width: "35%" }}>File Name</th>
                  <th style={{ width: "85px" }}>Status</th>
                  <th style={{ width: "85px" }} className="is-number">
                    Size
                  </th>
                  <th style={{ width: "160px" }}>Progress</th>
                  <th style={{ width: "95px" }} className="is-number">
                    Transfer Rate
                  </th>
                  <th style={{ width: "80px" }} className="is-number">
                    Time Left
                  </th>
                  <th style={{ width: "85px" }}>Category</th>
                  <th style={{ width: "90px" }}>Date Added</th>
                </tr>
              </thead>
              <tbody>
                {visible.length === 0 ? (
                  <tr>
                    <td colSpan={8} className="classic-empty">
                      <DownloadIcon className="h-8 w-8" />
                      <p>
                        {search
                          ? "No downloads match your search."
                          : "No downloads in this category."}
                      </p>
                      <button className="classic-button" onClick={() => openAdd()}>
                        Add a download
                      </button>
                    </td>
                  </tr>
                ) : (
                  visible.map((download) => {
                    const active = isActive(download.state);
                    const percent = Math.round(progressOf(download) * 100);
                    const category = categoryOfFilename(download.filename);
                    const eta =
                      active && download.eta_secs != null ? formatEta(download.eta_secs) : "—";
                    const addedDate = new Date(download.created_at).toLocaleDateString(undefined, {
                      month: "short",
                      day: "numeric",
                    });

                    return (
                      <tr
                        key={download.id}
                        className={selected === download.id ? "is-selected" : ""}
                        onClick={() => setSelected(download.id)}
                        onDoubleClick={() => handleRowDoubleClick(download)}
                        onContextMenu={(event) => rowMenu(event, download)}
                      >
                        <td>
                          <div className="classic-file">
                            <CategoryIcon category={category} className="h-4 w-4 shrink-0" />
                            <div className="min-w-0">
                              <span className="classic-file-name" title={download.filename}>
                                {download.filename || "Resolving filename…"}
                              </span>
                              <span className="classic-file-domain">
                                {domainOf(download.url)}
                                {download.accept_ranges ? " · Multi" : ""}
                              </span>
                            </div>
                          </div>
                        </td>
                        <td>
                          <span className={`classic-state state-${download.state}`}>
                            {STATE_LABEL[download.state]}
                          </span>
                        </td>
                        <td className="is-number">
                          {download.total_size ? formatBytes(download.total_size) : "—"}
                        </td>
                        <td>
                          <div className="classic-progress-cell">
                            <div className="classic-progress">
                              <span
                                className={`classic-progress-fill state-${download.state} ${
                                  download.state === "probing" || download.state === "completing"
                                    ? "is-indeterminate"
                                    : ""
                                }`}
                                style={{ width: `${Math.max(percent, 2)}%` }}
                              />
                            </div>
                            <span className="classic-percent">{formatPercent(download)}</span>
                          </div>
                        </td>
                        <td className="is-number">{active ? formatSpeed(download.speed) : "—"}</td>
                        <td className="is-number">{eta}</td>
                        <td className="capitalize">{category}</td>
                        <td className="text-[11px] text-slate-500">{addedDate}</td>
                      </tr>
                    );
                  })
                )}
              </tbody>
            </table>
          </div>
        </div>
      </div>

      {/* Bottom Status Bar */}
      <div className="classic-statusbar">
        <span className="classic-status-main">
          {stats.active > 0 ? "Downloading files with dynamic acceleration…" : "Ready"}
          {selectedDownload ? ` · ${selectedDownload.filename}` : ""}
        </span>
        <span>
          Speed: <strong>{formatSpeed(stats.speed)}</strong>
        </span>
        <span>Active: {stats.active}</span>
        <span>Queued: {stats.queued}</span>
        <span className="classic-status-grow" />
        <span>Total Received: {formatBytes(totalReceived)}</span>
        <span>HyperFetch 10x Turbo Engine</span>
      </div>

      {/* Right Click Context Menu */}
      {context ? (
        <div
          className="classic-context"
          style={{ left: context.x, top: context.y }}
          onClick={(event) => event.stopPropagation()}
        >
          {context.download.state === "done" ? (
            <button onClick={() => openFile(context.download)}>Open File</button>
          ) : null}
          <button onClick={() => openFolder(context.download)}>Open Folder</button>
          <button onClick={() => copyLink(context.download)}>Copy Download Link</button>
          <span className="classic-context-separator" />
          <button onClick={() => openProgressDialog(context.download.id)}>
            Download Status & Segments…
          </button>
          <button onClick={() => openProperties(context.download)}>Properties…</button>
          <span className="classic-context-separator" />
          {isActive(context.download.state) || context.download.state === "queued" ? (
            <button onClick={() => void pause(context.download.id)}>Pause</button>
          ) : null}
          {context.download.state === "paused" ||
          context.download.state === "error" ||
          context.download.state === "canceled" ? (
            <button onClick={() => void resume(context.download.id)}>Resume</button>
          ) : null}
          {isActive(context.download.state) || context.download.state === "queued" ? (
            <button onClick={() => void cancel(context.download.id)}>Cancel Transfer</button>
          ) : null}
          <span className="classic-context-separator" />
          <button onClick={() => void remove(context.download.id, false)}>Remove from List</button>
          {context.download.state === "done" ? (
            <button
              className="is-danger"
              onClick={() => {
                if (window.confirm(`Delete ${context.download.filename} from disk?`)) {
                  void remove(context.download.id, true);
                }
              }}
            >
              Delete File from Disk
            </button>
          ) : null}
          <span className="classic-context-separator" />
          <button onClick={() => setContext(null)}>Close Menu</button>
        </div>
      ) : null}
    </div>
  );
}
