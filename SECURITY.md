# Security policy

## Supported versions

HyperFetch is an alpha project. Security fixes are applied to the current `main` branch and the
latest release.

## Reporting a vulnerability

Please do not open a public issue for a suspected vulnerability. Use GitHub's private vulnerability
reporting on the repository security tab, or contact the maintainer through the repository profile.
Include the affected version, a minimal reproduction and the impact.

## Scope

Areas of particular interest:

- the loopback bridge (`127.0.0.1`) and bearer-token handling;
- browser extension permissions and capture forwarding;
- clipboard URL detection and automatic download starts;
- path handling for downloaded files and HLS output;
- Command execution through optional FFmpeg remuxing.

HyperFetch never bundles FFmpeg and never decrypts DRM-protected media. Reports about DRM
circumvention are out of scope.

## Local data

The SQLite database lives in the Tauri app data directory. The bridge token is stored in
`bridge.json` under the app config directory. Clipboard contents are processed locally and are not
uploaded anywhere.
