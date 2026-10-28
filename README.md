<p align="center">
  <img src="assets/icon-source.png" width="112" alt="HyperFetch logo" />
</p>

<h1 align="center">HyperFetch</h1>

<p align="center">
  A free, open-source, multi-connection download manager with browser capture, HLS video grabbing and site scanning.
  <br />
  Windows-first · Rust engine · Tauri 2 shell · MIT licensed
</p>

<p align="center">
  <a href="https://github.com/6t9xstar/hyperfetch-idm/actions/workflows/ci.yml"><img src="https://github.com/6t9xstar/hyperfetch-idm/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-16c98a" alt="MIT license" /></a>
  <img src="https://img.shields.io/badge/platform-Windows-0078d4" alt="Windows" />
</p>

---

HyperFetch is an original, clean-room download manager built from scratch. It accelerates HTTP(S)
downloads by splitting files into byte ranges across parallel connections, survives restarts with a
SQLite-backed resume system, and integrates with a browser extension through a token-protected local
bridge.

There is no IDM source code, binary, artwork or branding anywhere in this project, and there never
will be.

## Screenshots

<p align="center">
  <img src="assets/screenshots/classic.png" width="49%" alt="Classic HyperFetch layout with category tabs and dense download list" />
  <img src="assets/screenshots/modern.png" width="49%" alt="Modern HyperFetch dashboard with dark cards and live transfer table" />
</p>

<p align="center"><em>Classic download-manager layout and modern dashboard, both original HyperFetch designs.</em></p>

## Features

- **Accelerated downloads** — dynamic range splitting with automatic rebalancing; falls back to a
  single stream when a server ignores `Range`.
- **Crash-safe resume** — completed ranges and in-flight reservations are persisted and rebuilt on
  launch (`.hfpart` sidecar files).
- **Queue control** — configurable concurrency, per-file connection limits, global and per-file speed
  limits, pause / resume / cancel, retries with backoff.
- **Integrity** — validator-aware resuming (`If-Range`, ETag / Last-Modified) and optional SHA-256
  hashing on completion.
- **Browser integration** — Manifest V3 extension for Chrome and Firefox: context-menu capture, page
  link scanning, cookie/referer forwarding, bridge health checks.
- **Video grabber** — HLS parse (master/media playlists, `EXT-X-MAP` init sections for fragmented
  MP4), highest-bandwidth variant selection, standard AES-128 decryption, progress UI, optional
  FFmpeg remux to MP4.
- **Site grabber** — same-host crawling with depth/page limits and bulk queueing.
- **Two original themes** — a dense classic download-manager layout and a modern dark dashboard;
  switch live in Settings → Appearance.
- **Smart clipboard links** — copied file links are detected locally, de-duplicated and either added
  automatically or held for confirmation; ordinary web pages are ignored. HLS playlists are offered
  as grabber prompts.
- **Native desktop UI** — live transfer table, settings, capture confirmations and grabber activity.
- **Headless CLI + fixture server** — the engine is usable without the GUI and testable locally.

## Status

`v0.1.0` is an early alpha. Windows is the primary target; macOS and Linux builds should work with
the standard Tauri prerequisites but are not yet tested. DASH downloads are detected but not yet
implemented. DRM-protected content is explicitly out of scope.

## Quick start

### Prerequisites

- Rust **1.90+** with the MSVC toolchain on Windows
- Node.js **20+** and npm
- WebView2 runtime (preinstalled on Windows 10/11)
- Optional: `ffmpeg` on `PATH` for HLS → MP4 remuxing

### Desktop app

```bash
npm install
npm run tauri dev
```

`npm run tauri dev` starts Vite on `http://localhost:1420` and launches the Tauri shell.

### Production build

```bash
npm run tauri build
```

Installers are written to `target/release/bundle/` (NSIS `.exe` and MSI on Windows).

### Headless engine

```bash
cargo run -p hyperfetch-cli -- probe https://example.com/file.zip
cargo run -p hyperfetch-cli -- download https://example.com/file.zip --dir ./downloads
cargo run -p hyperfetch-cli -- list
```

Use `--db` to point at a specific SQLite database.

### Local test server

```bash
cargo run -p hyperfetch-fixture
# serves http://127.0.0.1:8080/file/10485760 (ranges),
#        /stream/<bytes> (zero-copy benchmark stream), /stats,
#        /norange/…, /slow/…, /drop/…, /etag/…
```

### Benchmark

The benchmark harness runs the CLI against the local fixture, resets its request counters for every
run, and verifies that exactly the requested number of bytes crossed the wire.

```bash
cargo build --release -p hyperfetch-cli
cargo run --release -p hyperfetch-fixture -- --port 8080
node scripts/bench.mjs --size 256 --runs 5 --connections 8 --batch 1048576
# matrix example:
node scripts/bench.mjs --connections 4,8,16 --batch 524288,1048576,4194304
```

On the development machine used for v0.1.0, a 256 MiB loopback stream measured roughly
**900–1,000 MiB/s median** with 8 connections and 1 MiB batches. These are local-fixture numbers,
not an internet-speed or IDM comparison; real servers vary with latency, disk and rate limits.

## Browser extension

Build both browser variants:

```bash
npm run extension:build
```

This writes `extension/build/chrome` and `extension/build/firefox`.

- **Chrome / Edge** — open `chrome://extensions`, enable Developer mode, choose **Load unpacked**
  and select `extension/build/chrome`.
- **Firefox** — open `about:debugging#/runtime/this-firefox`, choose **Load Temporary Add-on** and
  select `extension/build/firefox/manifest.json`.

Then pair the extension:

1. Start HyperFetch.
2. Open **Settings → Browser integration** in the app.
3. Copy the bridge URL and token (the token is also stored in
   `bridge.json` in the app config directory).
4. Open the extension popup, paste both values and press **Save and test connection**.

The bridge binds to `127.0.0.1` only and requires `Authorization: Bearer <token>` on every capture
endpoint. Captures can be confirmed in the app or auto-queued (disable *Ask before accepting browser
captures*).

## How it works

```
┌──────────────────────────────┐        ┌───────────────────────────┐
│  React + Tailwind UI (Vite)  │◀──────▶│  Tauri commands / events  │
└──────────────────────────────┘        └─────────────┬─────────────┘
                                                      │
┌──────────────────────────────┐        ┌─────────────▼─────────────┐
│  Browser extension (MV3)     │─HTTP──▶│  Loopback bridge (axum)   │
│  context menu + page scan    │ bearer │  /v1/health /v1/capture   │
└──────────────────────────────┘        └─────────────┬─────────────┘
                                                      │
                                        ┌─────────────▼─────────────┐
                                        │  hyperfetch-core engine   │
                                        │  ranges · queue · resume  │
                                        │  HLS · site crawler       │
                                        └─────────────┬─────────────┘
                                                      │
                                                 ┌────▼────┐
                                                 │ SQLite  │
                                                 └─────────┘
```

- The engine keeps an in-memory `RangePlan` per download. Workers reserve batch-sized ranges, write
  at absolute offsets into the sidecar file, and commit progress only after a successful batch.
- In-flight ranges are excluded from persistence; on restart they are re-queued, so no data is lost
  and no duplicate writes occur.
- Completed downloads are atomically renamed from `.hfpart` to the final name after optional hashing.
- Playlist and page crawling are handled by the same crate and exposed through Tauri commands.

## Repository layout

| Path | Purpose |
| --- | --- |
| `crates/core` | Download engine, SQLite store, probe, rate limiter, HLS/site grabbers |
| `crates/cli` | Headless `hyperfetch` CLI |
| `fixtures/range-server` | Axum server for range/fallback/retry tests |
| `src-tauri` | Tauri backend: commands, events, loopback bridge |
| `src` | React app (Zustand store, dialogs, download table) |
| `extension` | Chrome/Firefox MV3 extension |
| `scripts` | Icon generation and extension packaging |

## Testing

```bash
cargo test --workspace               # core, CLI, fixture, Tauri clipboard helpers
npm run typecheck
npm run build
npm run extension:build
```

Integration tests cover bit-exact segmented downloads, no-range fallback, pause/resume, crash
recovery and queue concurrency limits.

## Defaults

| Setting | Default |
| --- | --- |
| Simultaneous downloads | 3 |
| Connections per file | 8 |
| Batch size | 1 MiB |
| Minimum split size | 2 MiB |
| Minimum size for segmented mode | 4 MiB |
| Batch retries | 5 |
| SHA-256 on completion | Off |
| Durable final flush | Off |
| Clipboard monitoring | On |
| Auto-add clear clipboard files | On |
| Theme | Classic |
| Bridge port | 47821 (next free port used if busy) |

## Limitations

- **No DRM circumvention.** Widevine, PlayReady, FairPlay and similar protected streams are not
  supported and will not be.
- **HLS only for video grabbing.** Standard AES-128 keys referenced by the playlist, `EXT-X-MAP`
  init sections and TS/fMP4 segments are supported; DASH manifests are detected but not downloaded
  yet.
- **FFmpeg is optional.** If present, plain TS output is remuxed to MP4 without re-encoding. HyperFetch
  never bundles or links FFmpeg.
- **Same-host crawling.** The site grabber stays on the starting host and respects depth/page caps.
- **Alpha software.** Expect rough edges; do not rely on it as a backup tool yet.

## Security

- Bridge traffic is loopback-only and authenticated with a random token generated on first launch.
- The extension requests broad host permissions because it scans links and collects cookies for the
  current page; it sends data only to the configured `127.0.0.1` bridge.
- Clipboard links are parsed and probed locally. Clipboard contents are never uploaded; only clear
  file URLs or HLS playlists can start a download, and monitoring can be disabled in Settings.
- No telemetry, analytics or remote accounts. Nothing leaves your machine except the downloads you
  request.

## License

MIT. See [LICENSE](LICENSE).
