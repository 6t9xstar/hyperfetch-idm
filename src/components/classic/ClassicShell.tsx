import { useEffect, useState, type FormEvent, type MouseEvent as ReactMouseEvent } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { matchesFilter, useStore } from "../../store";
import type { DownloadSnapshot, Filter } from "../../lib/types";
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
  CategoryIcon,
  DownloadIcon,
  FilmIcon,
  FolderIcon,
  LinkIcon,
  PauseIcon,
  PlayIcon,
  PlusIcon,
  RefreshIcon,
  SearchIcon,
  SettingsIcon,
  TrashIcon,
} from "../Icons";

const TABS: { key: Filter; label: string }[] = [
  { key: "all", label: "All Downloads" },
  { key: "downloading", label: "Downloading" },
  { key: "queued", label: "Queued" },
  { key: "paused", label: "Paused" },
  { key: "finished", label: "Finished" },
  { key: "failed", label: "Failed" },
  { key: "programs", label: "Programs" },
  { key: "documents", label: "Documents" },
  { key: "audio", label: "Music" },
  { key: "video", label: "Video" },
  { key: "archives", label: "Compressed" },
  { key: "images", label: "Images" },
];

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
      .then(() => notify("Link copied", "success"))
      .catch((error) => notify(String(error), "error"));
  };

  const menus: { label: string; entries: MenuEntry[] }[] = [
    {
      label: "File",
      entries: [
        { label: "New Download…", action: () => openAdd() },
        { label: "Video Grabber…", action: () => setGrabberOpen(true) },
        { label: "Options…", action: () => setSettingsOpen(true) },
        { label: "Close Window", action: () => void getCurrentWindow().close() },
      ],
    },
    {
      label: "Downloads",
      entries: [
        { label: "Start All", action: () => void resumeAll() },
        { label: "Stop All", action: () => void pauseAll() },
        { label: "Refresh List", action: () => void refresh() },
        { label: "Clear Finished", action: () => void clearCompleted() },
      ],
    },
    {
      label: "View",
      entries: TABS.slice(0, 6).map((tab) => ({
        label: tab.label,
        action: () => setFilter(tab.key),
      })),
    },
    {
      label: "Help",
      entries: [
        {
          label: "About HyperFetch",
          action: () =>
            notify(
              "HyperFetch 0.1.0 · MIT licensed · original clean-room design",
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
      x: Math.min(event.clientX, window.innerWidth - 230),
      y: Math.min(event.clientY, window.innerHeight - 260),
      download,
    });
  };

  return (
    <div className="classic-root">
      <div className="classic-menubar" onClick={(event) => event.stopPropagation()}>
        <div className="classic-brand">
          <DownloadIcon className="h-4 w-4" />
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
        <div className="classic-menu-version">v0.1.0</div>
      </div>

      <div className="classic-toolbar">
        <button className="classic-tool" onClick={() => openAdd()} title="New download">
          <span className="classic-tool-icon">
            <PlusIcon className="h-7 w-7" />
          </span>
          <span>New</span>
        </button>
        <button className="classic-tool" onClick={() => void resumeAll()} title="Start all downloads">
          <span className="classic-tool-icon">
            <PlayIcon className="h-7 w-7" />
          </span>
          <span>Start</span>
        </button>
        <button className="classic-tool" onClick={() => void pauseAll()} title="Stop all downloads">
          <span className="classic-tool-icon">
            <PauseIcon className="h-7 w-7" />
          </span>
          <span>Stop</span>
        </button>
        <button
          className="classic-tool"
          disabled={!selectedDownload}
          onClick={() => {
            if (selectedDownload) openFolder(selectedDownload);
          }}
          title="Open the selected download folder"
        >
          <span className="classic-tool-icon">
            <FolderIcon className="h-7 w-7" />
          </span>
          <span>Folder</span>
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
          title="Remove the selected item from the list"
        >
          <span className="classic-tool-icon">
            <TrashIcon className="h-7 w-7" />
          </span>
          <span>Remove</span>
        </button>
        <span className="classic-toolbar-divider" />
        <button className="classic-tool" onClick={() => setGrabberOpen(true)} title="Video and site grabber">
          <span className="classic-tool-icon">
            <FilmIcon className="h-7 w-7" />
          </span>
          <span>Grabber</span>
        </button>
        <button className="classic-tool" onClick={() => void refresh()} title="Refresh list">
          <span className="classic-tool-icon">
            <RefreshIcon className="h-7 w-7" />
          </span>
          <span>Refresh</span>
        </button>
        <button className="classic-tool" onClick={() => setSettingsOpen(true)} title="Options">
          <span className="classic-tool-icon">
            <SettingsIcon className="h-7 w-7" />
          </span>
          <span>Options</span>
        </button>
      </div>

      <form className="classic-address" onSubmit={submitQuick}>
        <label className="classic-address-label" htmlFor="classic-url">
          <LinkIcon className="h-4 w-4" />
          URL:
        </label>
        <input
          id="classic-url"
          className="classic-address-input"
          value={quickUrl}
          onChange={(event) => setQuickUrl(event.target.value)}
          placeholder="Paste a download link here and press Enter"
          spellCheck={false}
        />
        <button className="classic-button" type="submit">
          Add URL
        </button>
        <div className="classic-search">
          <SearchIcon className="h-4 w-4" />
          <input
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            placeholder="Search downloads"
          />
        </div>
      </form>

      <div className="classic-tabs">
        {TABS.map((tab) => {
          const count = downloads.filter((download) => matchesFilter(download, tab.key)).length;
          return (
            <button
              key={tab.key}
              className={`classic-tab ${filter === tab.key ? "is-active" : ""}`}
              onClick={() => setFilter(tab.key)}
            >
              {tab.label}
              <span>({count})</span>
            </button>
          );
        })}
      </div>

      <div className="classic-table-wrap">
        <table className="classic-table">
          <thead>
            <tr>
              <th>File Name</th>
              <th>Status</th>
              <th className="is-number">Size</th>
              <th>Progress</th>
              <th className="is-number">Speed</th>
              <th className="is-number">Time Left</th>
            </tr>
          </thead>
          <tbody>
            {visible.length === 0 ? (
              <tr>
                <td colSpan={6} className="classic-empty">
                  <DownloadIcon className="h-8 w-8" />
                  <p>{search ? "No downloads match your search." : "No downloads in this category."}</p>
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
                const eta = active && download.eta_secs != null ? formatEta(download.eta_secs) : "—";
                return (
                  <tr
                    key={download.id}
                    className={selected === download.id ? "is-selected" : ""}
                    onClick={() => setSelected(download.id)}
                    onDoubleClick={() => {
                      if (download.state === "done") openFile(download);
                    }}
                    onContextMenu={(event) => rowMenu(event, download)}
                  >
                    <td>
                      <div className="classic-file">
                        <CategoryIcon category={category} className="h-4 w-4" />
                        <div>
                          <span className="classic-file-name" title={download.filename}>
                            {download.filename || "Resolving filename…"}
                          </span>
                          <span className="classic-file-domain">{domainOf(download.url)}</span>
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
                        <span>{formatPercent(download)}</span>
                      </div>
                    </td>
                    <td className="is-number">{active ? formatSpeed(download.speed) : "—"}</td>
                    <td className="is-number">{eta}</td>
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
      </div>

      <div className="classic-statusbar">
        <span className="classic-status-main">
          {stats.active > 0 ? "Downloading…" : "Ready"}
          {selectedDownload ? ` · ${selectedDownload.filename}` : ""}
        </span>
        <span>Speed: {formatSpeed(stats.speed)}</span>
        <span>Active: {stats.active}</span>
        <span>Queued: {stats.queued}</span>
        <span className="classic-status-grow" />
        <span>Received: {formatBytes(totalReceived)}</span>
        <span>HyperFetch 0.1.0</span>
      </div>

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
          <button onClick={() => copyLink(context.download)}>Copy Link</button>
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
              Delete File
            </button>
          ) : null}
          <span className="classic-context-separator" />
          <button onClick={() => setContext(null)}>Close Menu</button>
        </div>
      ) : null}
    </div>
  );
}
