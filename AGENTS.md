# Agent Instructions

- Use `bun` for JavaScript package management.
- Sabine runtime detection, downloads, validation, manifests, locks, pruning, and runtime paths live in `crates/sabine-runtime`.
- Shared app registration, runtime maintenance, and update policy live in `crates/sabine-service`.
- CEF host sources and cmake build for the shared embedder binary live in `crates/sabine-host`.
- CEF process launch, browser profiles, OSR transport, GPU composition, guests, and the public window API live in `crates/sabine`.
- Inside `crates/sabine/src`, domain folders are: `window/` (builder/config/origins/placement/listeners), `desktop/` (tray/shortcuts/autostart/single instance and the macOS desktop loop), `launch/` (host args/bootstrap/dev server), `host/` (process handles + profile paths), `bridge/` (host-side IPC wiring), `clipboard/` (native clipboard and drag-and-drop), `media/` (native video playback), `osr/` (protocol and desktop host), and `render/`.
- Sabine uses OSR on every desktop. Do not add windowed CEF, WebView2, or another renderer fallback.
- Sabine targets Linux on Wayland, Windows, and Apple Silicon macOS desktops. Do not add X11, mobile, or unsupported-platform no-op implementations.
- `sabine-platform` owns lightweight window/platform types, compositor regions, and native platform primitives.
- Apps should use `SabineWindow` from `sabine` directly.
- Prefer `SabineWindow::main`, recipes (`.app()` / `.palette()` / `.tray_app()`), `AppChrome`, and `with_manifest` for new apps; keep advanced region APIs available but secondary.
- Run `cargo fmt`, `cargo build --workspace`, and `cargo test --workspace` after code changes. Format C++ and Objective-C++ with `clang-format -i` using the repository `.clang-format`. For Windows-only changes, also run `cargo check --target x86_64-pc-windows-gnu --workspace` since the host development environment is typically Linux.
- Keep the README focused on getting started. Architecture and platform details belong in `docs/implementation-guide.md`; release procedures belong in `docs/publishing.md`.
- Preserve the warning banners around Windows-specific Chromium, GPU, and process behavior when editing those files.
- `.github/workflows/promote-release.yml` only promotes the newest release, and only once it is 24 hours old. Releases published less than a day apart keep postponing `latest` on purpose: it is a cooldown so users get one update after a busy stretch, not one per release. Don't "fix" it to promote older eligible releases.
- When publishing, use `scripts/publish.sh`. Crate metadata lives in each crate's `Cargo.toml`; the workspace owns version, license, repository, homepage, authors, keywords, categories.
- Never add Co-Authored-By, "Generated with", or any other AI or tool attribution lines to commit messages or pull request descriptions.
