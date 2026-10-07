# Unreleased

- Sabine runs only on Wayland on Linux. X11 sessions are no longer supported: the X11 clipboard and
  keyboard grab are gone, Chromium always uses its Wayland backend, and packages no longer depend
  on `libxkbcommon-x11`.
- Play native video on macOS through AVFoundation and on Windows through Media Foundation, with the
  page's own controls, subtitles, audio tracks and rounded corners, in transparent windows. On
  Windows, video frames reach the screen without redrawing the window. A subtitle's `end` is `null`
  on macOS, where it shows until the next one replaces or clears it.
- Drag and drop works like in a browser: drag within the window, drag out to other apps, and drop
  files, links and text in, with files in `dataTransfer.files`. The `window.fileDrag` event is gone;
  use the standard drag and drop events instead.
- Editable fields show a context menu with cut, copy, paste, select all and spelling suggestions.
- File inputs open the desktop's file dialog, attached to the window.
- Every CSS cursor works, including custom images and `cursor: none`.
- Command shortcuts for copy, cut, paste, undo, redo and select all work on macOS.
- Trackpad scrolling keeps sub-pixel motion, and pinching reaches pages as Ctrl+wheel. Pens hover
  pages, and touches lifted outside the page end properly.
- Minimized windows on Windows suspend their pages like covered windows on Wayland.
- Windows on Windows and macOS present without waiting for the display, and windows connect to
  their browser over a local socket instead of TCP.
- Hidden windows stop painting unless they keep their last frame, and GPU-shared frames copy only
  the area that changed.
- Guest previews can be captured on Windows and macOS.
- Tray icons behave the same on every desktop: left click opens the app, right click shows the
  menu, and menus can have submenus and checkboxes and change while the app runs. macOS menu bar
  icons no longer show the app's name and follow the menu bar's appearance.
- Global shortcuts accept any key, are requested together on Linux, and a shortcut another app
  holds no longer stops the app. Pages hear about refused shortcuts with `globalShortcut.failed`.
- Registering global shortcuts no longer hides the app's launcher entry on Linux.
- Tray icons and global shortcuts also work for apps that run their own event loop.
- Window regions keep working on Linux compositors without blur, and now work on macOS and Windows.
  Windows materials cover the whole window and follow the light or dark theme as it changes.
- Resize edges no longer take clicks in windows with system decorations.
- App-drawn titlebars keep the macOS traffic lights, and pages can tell which corner they cover
  with `appWindow.controlsOverlay()`.
- Clicking the Dock icon brings back a running macOS app's window.
- Pages can read and write any clipboard type on Windows and macOS.
- Pages can take the system's keyboard shortcuts on macOS once the app has Accessibility
  permission.
- Pages and Rust can show desktop notifications.
- Apps and pages can follow the desktop's light or dark preference and accent color.
- Fix pages losing their stored data when the cache folder is reached through a symbolic link:
  Chromium rejected the profile path and kept everything in memory.
- `skip_taskbar` and `always_on_top` work on Linux under Kestrel, so palettes and tray windows stay
  out of the taskbar and window switcher and above other windows there too.
- Async bridge handlers no longer occupy a bridge worker while they wait, so a slow handler cannot
  stall other commands, and Tokio timers and I/O work inside them.
- `NativeVideo.isSupported()` is false in windows that cannot show native video.
- The TypeScript helpers type every Sabine event, activity and guest result, and add listeners for
  tray, global shortcut, second-launch, renderer-crash and guest lifecycle events.
- Guests can navigate to inline HTML, `guest.list()` returns the guests themselves, and Rust can
  cover and uncover guests with `GuestHostControl::SetCovered`.
- The single-instance event reports its policy as `allowMultiple`, `reuseExisting` or
  `focusExisting`.
- The npm package includes the native video modules it imports.
- `cargo run` finds the app's `Sabine.toml` from anywhere in a workspace.
- Remove bridge command permissions, which the app granted to itself and so protected nothing.
- Uninstalling an app removes its login item, browser extension hosts, staged updates and
  downloads, on every platform, and installed update packages are deleted once applied.
- Upgrading an MSI package no longer removes the app's browser extension hosts.
- Apps can list file extensions for their document types under `[app.extensions]`: Windows offers
  them under Open with, macOS declares them by type identifier, and Linux packages teach the desktop
  the extensions.
- deb, rpm and AppImage packages install themed icons and AppStream metadata, and RPMs install on
  openSUSE and other RPM distributions.
- `.exe` updates install silently, and macOS asks for an administrator only when the app cannot be
  replaced otherwise.
- `sabine bundle` signs and notarizes macOS apps and signs Windows apps and installers when signing
  is configured.
- Releases include ARM64 Linux and Windows builds, Windows setup executables, and updates for apps
  installed with `sabine install --bundle`.
- Register browser native messaging hosts for Chrome Beta, Dev and Canary, Edge and Brave channels,
  Vivaldi Snapshot, LibreWolf, and Flatpak and Snap browsers.
- The Sabine service starts at login on Linux sessions without systemd.
- Sabine requires CEF 154 or newer.
- Apps built before Sabine recorded compatibility metadata, and release metadata signed in the old
  format, are no longer accepted. Rebuild such apps with this version.
- Keep apps that only open from their URL schemes or documents out of launchers with
  `listed = false` under `[app]` in `Sabine.toml`. They stay installed and keep their handlers:
  Linux desktop entries are marked `NoDisplay`, Windows installs skip the Start menu shortcut, and
  `sabine install` on macOS registers the bundle with Launch Services instead of placing it in
  `~/Applications`.
- Windows installers register an app's URL schemes during installation, so links open it before
  its first launch, and remove them when it is uninstalled.
- `sabine uninstall` removes the URL scheme and file type registrations an app made: its scheme keys
  on Windows, its hidden URL handler and `mimeapps.list` defaults on Linux, and its Launch Services
  record on macOS. Registrations that another installation has since taken over are kept, and
  Windows `.exe` uninstallers now leave such schemes alone too.
- `sabine uninstall` and Windows `.exe` and MSI uninstallers remove the browser native messaging
  manifests an app wrote, and on Windows the registry keys pointing to them, as long as they still
  launch that installation.
- `sabine uninstall` without `--purge` no longer leaves the installation's file list and an empty
  installation folder behind. Browser profiles and files people added to the folder are still kept.
- Linux URL handlers name their program in `TryExec`, so desktops ignore a handler left behind after
  a deb, rpm or AppImage package is removed.
- Register browser native messaging hosts for Vivaldi, Opera and Helium as well, and remove them
  again on uninstall. Zen already reads the Firefox manifests.
- Fix URL handlers and native messaging hosts of apps run from an AppImage pointing into the
  AppImage's temporary mount, which disappears when the app exits. Handlers now launch the AppImage
  file, and hosts inside it start through a launcher that runs them from the AppImage, which needs
  the app to be rebuilt with this version. Sabine's record of the app names the AppImage file too.
- Fix Linux apps crashing at startup, over and over, when the desktop's settings portal is not
  running. Chromium's GTK 4 integration called into GTK 3 when it found no text scaling setting,
  so Sabine now has Chromium use GTK 3, which reads the desktop's settings without the portal.

# Sabine 0.33

- Pause pages while the desktop hides their window. On Wayland compositors that mark minimized or
  covered windows as suspended, such as Kestrel, and on X11 when a window is fully covered,
  Chromium stops rendering the page and throttles its timers until the window is seen again; a
  minimized Barometer went from 7.5% of a core to 0.4%. Pages can follow the window's state with
  `appWindow.visible`, `appWindow.suspended` and `appWindow.onVisibilityChanged` from
  `@lantharos/sabine`, and Rust code with `SabineWindow::on_visibility_changed`. The internal
  `window.__sabineLifecycleSet` hook is gone.
- Fix Chromium spinning at full speed after a window was minimized or hidden on Linux. The window
  cancelled an input method composition even when none was open, which set the page and browser
  processes messaging each other without end, at about one and a half cores, even after the window
  was shown again.
- Paint software frames with one copy instead of two. The window writes each changed region from
  Chromium's shared memory straight into its GPU texture rather than patching a full copy of the
  page first, which cut the window's CPU time by about a fifth while scrolling a large folder in
  Rover, and it no longer keeps that copy of every window's pixels in memory.
- Only the primary mouse button moves or resizes a window from drag regions, resize edges and
  window controls; other buttons reach the page, so right-clicking empty space in a draggable
  sidebar no longer starts a window move.

# Sabine 0.32

- Fix Linux pages rendering without Chromium's built-in styles, which showed the contents of
  `<head>` as text: renderers could not find `resources.pak` and `chrome_100_percent.pak` beside
  the runtime library. Existing runtimes are repaired on the next launch or update.
- Play H.264, HEVC, AAC and other media Chromium cannot decode through `NativeVideo` on Linux. Video
  is decoded in hardware where available and shown on a native surface beneath the page, with the
  page's own controls, subtitles, audio tracks and fullscreen.
- Let launchers find apps by other names: `generic_name`, `categories` and `keywords` under `[app]`
  in `Sabine.toml` go into desktop entries, with `categories` replacing the fixed `Utility`.
- Keep windows responsive while they are not being shown: presenting now waits for the compositor
  to ask for a frame instead of blocking the window until the display catches up, and Linux windows
  present in mailbox mode where the driver supports it. Bridge calls from animating pages return
  within a millisecond instead of one display frame, and out-of-view windows no longer stall for up
  to a second on every paint.
- Fix copy and paste on Linux: pages now use the desktop clipboard for Ctrl+C, Ctrl+X, Ctrl+V,
  Ctrl+Shift+V, `navigator.clipboard` and cut, including HTML and images, and selecting text or
  middle-clicking uses the primary selection. The app's own pages can read the clipboard without
  a prompt. `clipboard.read()` and `clipboard.write()` in `@lantharos/sabine` take any MIME type,
  such as a file manager's copied files.
- Change a window's blur, opaque and input regions while it runs, with `set_regions` and
  `set_regions_of` in Rust or `appWindow.setRegions` in pages, so an app can drop a hidden
  sidebar's blur without restarting.
- Send events to one window with `emit_to` and `emit_bytes_to`. Bridge commands name the window that
  called them in `command.window`, so apps with several windows can answer only that one.
- Sending events now waits while a window catches up instead of dropping them once 512 are queued,
  and a burst of events no longer closes a window's connection to its pages.
- Move bytes between pages and the app without JSON or base64: calls can carry a `body`, handlers
  can answer with `BridgeResponse::bytes`, and `emit_bytes` streams bytes such as terminal output.
  Pages receive a `Uint8Array`.
- Choose the DevTools port of development runs with `SABINE_DEVTOOLS_PORT`, so several apps in
  development no longer collide on 9222. `0` picks a free port.
- Add `appWindow.inhibitShortcuts(enabled)` so pages that record keyboard shortcuts receive keys the
  desktop normally keeps for itself, such as the Super key or Alt+Tab. It works on Wayland, X11 and
  Windows and rejects on macOS.

# Sabine 0.31

- Fix typing punctuation in pages: characters such as `.`, `-`, `$`, `#`, `%` and `'` were sent to
  Chromium as the Delete, Insert, Home, End and arrow keys, deleting text or moving the caret. Keys
  now report the same `key`, `code` and `keyCode` values as browsers.
- Fix Enter in text areas and editable content, which inserted nothing and could not submit forms.
- Report `event.code` for every key, and include Shift, Control, Alt, Meta, Caps Lock, Insert and
  other named keys that pages previously received as unidentified keys. Modifier keys now report
  their own modifier state the way browsers do.
- Apps that the installed Sabine no longer supports fetch their newest release right away instead of
  waiting for the soak period, and offer a downloaded update before showing the incompatibility
  notice.

# Sabine 0.30

Apps built before this release must be rebuilt: the host protocol changed, so the minimum
supported app build rises to this release. Local pages now load from `sabine://app/`, so data a page
kept in browser storage under its old `file://` origin does not carry over.

- Serve local app files from `sabine://app/`, a secure origin limited to the entry's directory.
  Pages can no longer read other files on disk, byte ranges are supported for media, and every page
  under the app origin keeps bridge access, including multi-page apps and client-side routes.
- Let apps opt into local file access with `local_files = true` under `[web]` or
  `.local_files(true)`, and build file URLs with `fileUrl` from `@lantharos/sabine`.
- Add accelerated painting on macOS through shared IOSurfaces, with no copy through system memory.
- Import each accelerated paint slot once on macOS and Windows instead of on every frame; Windows
  shares each texture handle once rather than duplicating it per frame.
- Show the first frame about 25% sooner on Linux by no longer triggering Chromium's Vulkan driver
  probe, and stop Chromium's background on-device AI benchmark.
- Start Chromium alongside graphics setup and verify the host when it connects.
- Pass bridge requests and responses as structured values: large payloads return about twice as
  fast, and non-ASCII payloads can use the full 1 MiB limit.
- Rework the software paint transport around reusable shared memory, lowering Chromium CPU use, and
  fix HiDPI surface sizing.
- Fix Windows page renderers exiting at startup when `libcef.dll` could not be found; every
  Chromium process now loads it from the runtime by path.
- Stop macOS from asking for the login keychain password: Chromium uses its mock keychain, and the
  runtime probe never touches the user's credential store.
- Check accelerated painting on macOS and Windows runners with the manual Paint check workflow,
  and run Clippy on the macOS and Windows CI jobs.

# Sabine 0.29

- Remove the Wayland layer-shell backend, broker, and public shell-surface API to focus Sabine on
  desktop windows.

- Add software-rendered error details, recent log summaries, an Open logs action, keyboard controls,
  and emergency system notifications when the diagnostic window cannot open.
- Separate development app registrations, browser profiles, and native data paths from production.
  Source installs launch the configured development server and can coexist with production bundles.
- Add manual component updates, production bundle installs, and app/system uninstall commands.
  Force updates bypass soak time while retaining signature, integrity and version checks.
- Pair Windows host launches with the selected CEF bootstrap to prevent crashes after CEF updates.
- Show CLI, installed service and running daemon versions with `sabine -V`.
- Automate release version references, checks, signed commits/tags, and workflow monitoring.

# Sabine 0.28

- Windows shared-texture rendering preserves pixel alignment instead of shrinking the
  sampled frame by one pixel, keeping text and fine details sharp at native resolution.

# Sabine 0.27

- Windows OSR connections switch accepted sockets to blocking reads before authentication,
  preventing startup disconnections when Chromium pauses between messages.
- Windows prepares Chromium resource packs beside its runtime DLLs as well as ICU data.
- Chromium uses D3D11 WARP when the Windows renderer selects a software adapter, including
  virtual machines without GPU acceleration. Hardware adapters retain D3D11, and both paths
  keep shared-texture rendering.
- Accelerated frames dismiss the loading screen when page loading finishes before the
  first shared texture arrives.
- Windows installer checks require the installed app to present a Chromium frame and keep
  its OSR connection alive. Missing resource packs and connection recovery fail validation.

# Sabine 0.26

- Previously downloaded app updates are only offered or installed while their version is newer
  than the installed app. Manually installing a newer app makes an older pending update ineligible.
- Windows prepares ICU data beside the Chromium library before launch, fixing startup crashes
  reporting an invalid ICU data file descriptor.
- Host protocol checks allow up to 30 seconds for cold starts on slower machines and virtual
  machines. Successful checks return immediately.
- Windows package and app release checks require Chromium to render and verify a probe page,
  in addition to checking installation and background-service startup.
- App release workflows accept the application's Rust toolchain and install the dependencies
  needed to build the matching CLI and publish signed update manifests.

Windows MSI cancellation, installation, repair, browser rendering, and uninstall passed on a
native Windows runner. Workspace builds, tests, strict Clippy, and Windows cross-compilation
are required before publishing. The minimum supported app build remains 23.

# Sabine 0.25

Apps built before Sabine 0.23 must be rebuilt before using this shared system. The minimum
supported app build is 23 because the bridge authorization and host protocol changed. Existing app registrations
remain available for updates. macOS releases support Apple Silicon; Intel Mac targets have been
removed. The minimum Rust version is 1.90.

- Windows EXE and MSI installers prepare the shared runtime inside the installer wizard, report
  setup failures, support cancellation and repair, and roll back failed setup. EXE upgrades remove
  obsolete application files while preserving user-created files.
- Windows background processes run without console windows. Native process ownership, shutdown,
  bounded diagnostic logs, and visible startup errors make failed launches easier to diagnose.
- Chromium runs with its sandbox on Linux, Windows, and macOS. Native bridge commands are bound to
  authorized documents, and unsupported permission requests resolve with a denial. Linux provides
  an AppArmor profile command for systems that restrict unprivileged namespaces.
- Rendering follows the active display's refresh rate. Bounded paint queues, shared GPU instance
  lifetime, frame retirement, and device-loss recovery reduce retained resources and idle work.
  Hidden-window hibernation releases Chromium and per-window GPU resources.
- Runtime and application updates use operating-system locks, recoverable publication, validated
  archive extraction, and bounded resumable downloads. App updates remain available when an older
  app cannot use the current shared system. Daemon ownership is serialized across concurrent starts.
- Linux packages have private application payloads, native dependency metadata, and corrected
  desktop registration. Icons are generated in process, including SVG, Windows ICO, and macOS ICNS.
- Apple Silicon bundles use sandbox-compatible CEF launch bundles without modifying the original
  host bundle. URL and document delivery works at startup and in an existing application.
- Guest shortcut, wheel, and download options agree across Rust and JavaScript. The typed package
  includes cancellation, request deadlines, open-URL delivery, and native Save-dialog options.
  Chromium and Firefox native messaging use their respective manifest and registration formats.

Validation covers workspace builds, tests, strict Clippy, Windows cross-compilation, native CEF
rendering and shutdown on all three desktop platforms, Linux package formats, macOS bundles, and
Windows installer cancellation, install, repair, launch, and uninstall. EXE upgrade checks also
verify user-file preservation. Linux runtime checks cover guest lifetime, recovery, sandboxing,
input forwarding, bridge permissions, downloads, hibernation, and concurrent daemon startup.

Native screen-reader integration is not implemented. Linux and macOS browser dialogs use the
system's default ownership. Physical Windows/macOS IME and hybrid-GPU behavior still need device
testing; CI uses hosted machines. macOS distribution signing and notarization remain the app
publisher's responsibility.

The v0.23 and v0.24 tags remain available for source history; neither produced a published release.
Version 0.25 includes the external-host AppArmor correction caught during v0.23 validation and uses
the tested Linux CLI to generate release metadata, avoiding the redundant build that stopped v0.24
publication. Use v0.25 when updating application dependencies.
