import { useEffect, useState, type ReactNode } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useStore } from "../store";
import type { Settings } from "../lib/types";
import { Modal } from "./Modal";
import { CopyIcon, FolderIcon, GlobeIcon, PulseIcon, SettingsIcon, ZapIcon } from "./Icons";

function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: ReactNode;
}) {
  return (
    <div>
      <span className="label">{label}</span>
      {children}
      {hint ? <p className="mt-1 text-[10px] leading-relaxed text-slate-500">{hint}</p> : null}
    </div>
  );
}

function Section({
  title,
  description,
  children,
}: {
  title: string;
  description: string;
  children: ReactNode;
}) {
  return (
    <section className="border-b border-white/[0.05] pb-5 last:border-0 last:pb-0">
      <h3 className="text-sm font-semibold text-slate-100">{title}</h3>
      <p className="mb-3 mt-0.5 text-[11px] text-slate-500">{description}</p>
      <div className="space-y-3">{children}</div>
    </section>
  );
}

function Toggle({
  label,
  hint,
  checked,
  onChange,
}: {
  label: string;
  hint: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className="flex cursor-pointer items-start gap-2.5 rounded-lg border border-white/[0.05] bg-white/[0.02] px-3 py-2.5">
      <input
        type="checkbox"
        className="mt-0.5 h-4 w-4 rounded border-white/20 bg-ink-800 accent-accent-500"
        checked={checked}
        onChange={(event) => onChange(event.target.checked)}
      />
      <span>
        <span className="block text-xs font-medium text-slate-200">{label}</span>
        <span className="mt-0.5 block text-[10px] leading-relaxed text-slate-500">{hint}</span>
      </span>
    </label>
  );
}

export function SettingsDialog() {
  const openState = useStore((state) => state.settingsOpen);
  const setSettingsOpen = useStore((state) => state.setSettingsOpen);
  const settings = useStore((state) => state.settings);
  const saveSettings = useStore((state) => state.saveSettings);
  const notify = useStore((state) => state.notify);
  const bridge = useStore((state) => state.bridge);

  const [draft, setDraft] = useState<Settings | null>(null);
  const [showToken, setShowToken] = useState(false);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (openState && settings) {
      setDraft(JSON.parse(JSON.stringify(settings)) as Settings);
      setShowToken(false);
    }
  }, [openState, settings]);

  if (!draft) {
    return <Modal open={openState} onClose={() => setSettingsOpen(false)} title="Settings" children={null} />;
  }

  const update = <K extends keyof Settings>(key: K, value: Settings[K]) =>
    setDraft((current) => (current ? { ...current, [key]: value } : current));

  const pickFolder = async () => {
    const selected = await open({ directory: true, defaultPath: draft.download_dir || undefined });
    if (typeof selected === "string") update("download_dir", selected);
  };

  const copy = async (value: string, label: string) => {
    try {
      await navigator.clipboard.writeText(value);
      notify(`${label} copied`, "success");
    } catch {
      notify("Clipboard unavailable", "error");
    }
  };

  const save = async () => {
    setSaving(true);
    const ok = await saveSettings(draft);
    setSaving(false);
    if (ok) setSettingsOpen(false);
  };

  return (
    <Modal
      open={openState}
      onClose={() => setSettingsOpen(false)}
      title="Settings"
      subtitle="Tune transfers, integrations and storage behaviour."
      width="max-w-2xl"
      footer={
        <>
          <button className="btn" onClick={() => setSettingsOpen(false)}>
            Cancel
          </button>
          <button className="btn btn-primary min-w-24" onClick={() => void save()} disabled={saving}>
            {saving ? "Saving…" : "Save settings"}
          </button>
        </>
      }
    >
      <div className="space-y-5">
        <Section
          title="Appearance"
          description="Choose the familiar dense layout or the modern dashboard."
        >
          <div className="grid gap-3 sm:grid-cols-2">
            {[
              {
                id: "classic",
                label: "Classic",
                hint: "Menu bar, category tabs and a dense download list.",
              },
              {
                id: "modern",
                label: "Modern",
                hint: "Dark dashboard with focused cards and spacing.",
              },
            ].map((option) => {
              const active =
                option.id === "classic"
                  ? draft.theme === "classic"
                  : draft.theme !== "classic";
              return (
                <button
                  key={option.id}
                  type="button"
                  onClick={() => update("theme", option.id)}
                  className={`rounded-xl border p-3 text-left transition ${
                    active
                      ? "border-accent-500/60 bg-accent-500/10"
                      : "border-white/10 bg-white/[0.02] hover:border-white/20"
                  }`}
                >
                  <span
                    className={`classic-preview ${option.id === "classic" ? "is-classic" : ""}`}
                  >
                    <span className="classic-preview-bar" />
                    <span className="classic-preview-row" />
                    <span className="classic-preview-row is-short" />
                    <span className="classic-preview-row" />
                  </span>
                  <span className="mt-2 block text-xs font-semibold text-slate-200">
                    {option.label}
                  </span>
                  <span className="mt-0.5 block text-[11px] leading-relaxed text-slate-500">
                    {option.hint}
                  </span>
                </button>
              );
            })}
          </div>
        </Section>

        <Section
          title="Downloads"
          description="Where files land and how aggressively transfers are parallelised."
        >
          <Field label="Download folder">
            <div className="flex gap-2">
              <div className="relative flex-1">
                <FolderIcon className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-slate-500" />
                <input
                  className="input pl-9 font-mono text-[12px]"
                  value={draft.download_dir}
                  onChange={(event) => update("download_dir", event.target.value)}
                />
              </div>
              <button className="btn shrink-0" onClick={() => void pickFolder()}>
                Browse
              </button>
            </div>
          </Field>
          <div className="grid gap-3 sm:grid-cols-2">
            <Field label="Simultaneous downloads" hint="Active transfers at the same time.">
              <input
                type="number"
                min={1}
                max={10}
                className="input"
                value={draft.max_concurrent}
                onChange={(event) =>
                  update("max_concurrent", Math.max(1, Number(event.target.value) || 1))
                }
              />
            </Field>
            <Field label="Connections per file" hint="Segments requested in parallel per download.">
              <input
                type="number"
                min={1}
                max={32}
                className="input"
                value={draft.connections_per_download}
                onChange={(event) =>
                  update(
                    "connections_per_download",
                    Math.max(1, Number(event.target.value) || 1),
                  )
                }
              />
            </Field>
          </div>
          <Toggle
            label="Sort files into category folders"
            hint="Video, Audio, Documents and other folders are created automatically inside the download folder."
            checked={draft.categorize}
            onChange={(value) => update("categorize", value)}
          />
          <Toggle
            label="Resume unfinished downloads on launch"
            hint="Rebuild the queue from the database and continue interrupted transfers."
            checked={draft.auto_resume}
            onChange={(value) => update("auto_resume", value)}
          />
          <Toggle
            label="Compute SHA-256 on completion"
            hint="Useful for verifying downloads; adds a short finalize pass."
            checked={draft.compute_sha256}
            onChange={(value) => update("compute_sha256", value)}
          />
          <Toggle
            label="Flush file metadata durably"
            hint="Safer if the machine loses power during finalization; skips a fast disk sync when off."
            checked={draft.durable_finish}
            onChange={(value) => update("durable_finish", value)}
          />
        </Section>

        <Section
          title="Clipboard"
          description="HyperFetch can watch copied links locally. Clipboard text never leaves this device."
        >
          <Toggle
            label="Watch clipboard for download links"
            hint="Detect copied file links and HLS playlists automatically."
            checked={draft.clipboard_monitor}
            onChange={(value) => update("clipboard_monitor", value)}
          />
          <Toggle
            label="Start clear file links automatically"
            hint="When off, detected files wait for confirmation in Incoming links. Web pages are always ignored."
            checked={draft.clipboard_auto_add}
            onChange={(value) => update("clipboard_auto_add", value)}
          />
        </Section>

        <Section
          title="Network"
          description="Global bandwidth limits and request identity."
        >
          <div className="grid gap-3 sm:grid-cols-2">
            <Field label="Global speed limit (MB/s)" hint="0 = unlimited. Applies to all transfers.">
              <input
                type="number"
                min={0}
                step={0.5}
                className="input"
                value={draft.global_speed_limit / (1024 * 1024)}
                onChange={(event) =>
                  update(
                    "global_speed_limit",
                    Math.round(Math.max(0, Number(event.target.value) || 0) * 1024 * 1024),
                  )
                }
              />
            </Field>
            <Field label="Retries per failed chunk" hint="Range batches are retried before failing.">
              <input
                type="number"
                min={0}
                max={20}
                className="input"
                value={draft.batch_retries}
                onChange={(event) =>
                  update("batch_retries", Math.max(0, Number(event.target.value) || 0))
                }
              />
            </Field>
          </div>
          <Field label="HTTP proxy" hint="Optional, e.g. http://127.0.0.1:8080. Leave empty for direct connections.">
            <input
              className="input font-mono text-[12px]"
              value={draft.proxy ?? ""}
              placeholder="none"
              onChange={(event) => update("proxy", event.target.value || null)}
            />
          </Field>
          <Field label="User agent">
            <input
              className="input font-mono text-[12px]"
              value={draft.user_agent}
              onChange={(event) => update("user_agent", event.target.value)}
            />
          </Field>
        </Section>

        <Section
          title="Browser integration"
          description="The local bridge lets the HyperFetch extension send links to the desktop app."
        >
          <Toggle
            label="Ask before accepting browser captures"
            hint="Show a confirmation card instead of starting downloads immediately."
            checked={draft.confirm_captures}
            onChange={(value) => update("confirm_captures", value)}
          />
          <div className="rounded-lg border border-white/[0.06] bg-ink-950/50 p-3">
            <div className="flex items-center gap-2 text-xs text-slate-300">
              <GlobeIcon className="h-4 w-4 text-accent-400" />
              <span className="font-medium">Loopback bridge</span>
              <span
                className={`ml-auto h-1.5 w-1.5 rounded-full ${
                  bridge ? "bg-accent-400" : "bg-slate-600"
                }`}
              />
            </div>
            <div className="mt-2 space-y-1.5 font-mono text-[11px]">
              <div className="flex items-center gap-2">
                <span className="w-10 shrink-0 text-slate-500">URL</span>
                <span className="flex-1 truncate text-slate-300">{bridge?.url ?? "offline"}</span>
                {bridge ? (
                  <button
                    className="icon-btn h-6 w-6"
                    title="Copy bridge URL"
                    onClick={() => void copy(bridge.url, "Bridge URL")}
                  >
                    <CopyIcon className="h-3.5 w-3.5" />
                  </button>
                ) : null}
              </div>
              <div className="flex items-center gap-2">
                <span className="w-10 shrink-0 text-slate-500">Token</span>
                <span className="flex-1 truncate text-slate-300">
                  {showToken ? bridge?.token : "••••••••••••••••••••••••"}
                </span>
                <button
                  className="icon-btn h-6 w-6"
                  title={showToken ? "Hide token" : "Reveal token"}
                  onClick={() => setShowToken((value) => !value)}
                >
                  <SettingsIcon className="h-3.5 w-3.5" />
                </button>
                {bridge ? (
                  <button
                    className="icon-btn h-6 w-6"
                    title="Copy token"
                    onClick={() => void copy(bridge.token, "Bridge token")}
                  >
                    <CopyIcon className="h-3.5 w-3.5" />
                  </button>
                ) : null}
              </div>
            </div>
            <p className="mt-2 text-[10px] leading-relaxed text-slate-500">
              Requests are authenticated with a bearer token and bound to 127.0.0.1 only.
              The extension pairs automatically when both are installed.
            </p>
          </div>
          <Field
            label="Bridge port"
            hint="Change requires restarting HyperFetch. The next free port is used if this one is busy."
          >
            <input
              type="number"
              min={1024}
              max={65535}
              className="input w-40"
              value={draft.bridge_port}
              onChange={(event) =>
                update("bridge_port", Math.max(1024, Number(event.target.value) || 47821))
              }
            />
          </Field>
        </Section>

        <Section title="About" description="HyperFetch is an open-source download manager.">
          <div className="grid gap-2 text-[11px] text-slate-400 sm:grid-cols-2">
            <div className="flex items-center gap-2 rounded-lg border border-white/[0.05] px-3 py-2">
              <ZapIcon className="h-4 w-4 text-accent-400" />
              <span>Version 0.1.0 · MIT license</span>
            </div>
            <div className="flex items-center gap-2 rounded-lg border border-white/[0.05] px-3 py-2">
              <PulseIcon className="h-4 w-4 text-sky-400" />
              <span>Rust engine · Tauri 2 shell</span>
            </div>
          </div>
          <p className="text-[10px] leading-relaxed text-slate-500">
            HLS download is supported, including standard AES-128 keys from the playlist. DASH
            manifests are detected but not yet downloadable. DRM-protected streams are never
            supported. FFmpeg is optional and used only to remux plain HLS segments.
          </p>
        </Section>
      </div>
    </Modal>
  );
}
