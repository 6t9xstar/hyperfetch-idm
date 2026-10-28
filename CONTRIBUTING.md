# Contributing to HyperFetch

Thanks for helping improve HyperFetch. The project is an original, MIT-licensed download manager;
please do not submit code, assets or text copied from IDM or any other proprietary product.

## Development setup

Prerequisites: Rust 1.90+ (MSVC on Windows), Node.js 20+, npm and the Tauri 2 platform
dependencies. WebView2 is required on Windows.

```bash
npm install
npm run tauri dev
```

Run the fixture server in a second terminal when working on download behavior:

```bash
cargo run -p hyperfetch-fixture
```

## Checks before opening a pull request

```bash
cargo fmt --all -- --check
cargo clippy -p hyperfetch-core -p hyperfetch-cli -p hyperfetch-fixture --all-targets -- -D warnings
cargo test --workspace
npm run typecheck
npm run build
npm run extension:build
```

For engine changes, include a regression test that fails before the fix and passes after it. For UI
changes, attach a screenshot of both themes if the affected screen is theme-sensitive.

## Pull requests

- Keep commits focused and describe the user-visible behavior.
- Follow the existing Rust and TypeScript style; avoid new dependencies unless necessary.
- Update `README.md`, defaults or fixtures when behavior changes.
- Do not include downloaded media, databases, bridge tokens or other local state.

## Security

Please report vulnerabilities privately as described in [SECURITY.md](SECURITY.md), not in a public
issue.
