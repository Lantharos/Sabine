# Agent Instructions

- Use `bun` for JavaScript package management.
- Sabine runtime detection, downloads, validation, manifests, locks, pruning, and runtime paths live in `crates/sabine-runtime`.
- Shared app registration, runtime maintenance, and update policy live in `crates/sabine-service`.
- CEF host sources and cmake build for the shared embedder binary live in `crates/sabine-host`.
- CEF process launch, browser profiles, OSR transport, GPU composition, guests, and the public window API live in `crates/sabine`.
- Inside `crates/sabine/src`, domain folders are: `window/` (builder/config/glass), `desktop/` (tray/shortcuts/autostart), `launch/` (host args/bootstrap), `host/` (process handles + profile paths), `bridge/` (host-side IPC wiring), `osr/` (protocol and desktop host), and `render/`.
- Sabine uses OSR on every desktop. Do not add windowed CEF, WebView2, or another renderer fallback.
- Sabine targets Linux, Windows, and Apple Silicon macOS desktops. Do not add mobile or unsupported-platform no-op implementations.
- `sabine-platform` owns lightweight window/platform types, compositor regions, and native platform primitives.
- Apps should use `SabineWindow` from `sabine` directly.
- Prefer `SabineWindow::main`, recipes (`.app()` / `.palette()` / `.tray_app()`), `AppChrome`, and `with_manifest` for new apps; keep advanced region APIs available but secondary.
- Run `cargo fmt`, `cargo build --workspace`, and `cargo test --workspace` after code changes. For Windows-only changes, also run `cargo check --target x86_64-pc-windows-gnu --workspace` since the host development environment is typically Linux.
- Keep the README focused on getting started. Architecture and platform details belong in `docs/implementation-guide.md`; release procedures belong in `docs/publishing.md`.
- Preserve the warning banners around Windows-specific Chromium, GPU, and process behavior when editing those files.
- When publishing, use `scripts/publish.sh`. Crate metadata lives in each crate's `Cargo.toml`; the workspace owns version, license, repository, homepage, authors, keywords, categories.
