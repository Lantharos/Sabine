# Web API

Pages running inside a Sabine window talk to the app through `@lantharos/sabine`. Prefer these
exports over reaching for `window.sabine` directly.

## Bridge commands

```js
import { invoke, listen, events, appWindow } from "@lantharos/sabine";

const { version } = await invoke("app.version");
listen("notes.changed", (note) => console.log(note));

events.trayActivate(({ itemId }) => {
  if (itemId === null) appWindow.show();
});

events.guestDownload((download) => {
  console.log(download.filename, download.state);
});

events.guestShortcut((event) => {
  console.log(event.accelerator, event.key);
});

events.guestWheel((event) => {
  console.log(event.deltaX, event.deltaY);
});

events.guestFavicon((event) => {
  console.log(event.id, event.favicons);
});
```

Requests accept an `AbortSignal` and a deadline in milliseconds:

```js
const controller = new AbortController();
const result = invoke("app.search", { query: "notes" }, {
  signal: controller.signal,
  timeoutMs: 10000,
});
controller.abort();
```

The default deadline is one minute. Aborting, timing out, or leaving the page releases the pending
request and ignores later responses. This does not interrupt a Rust handler that is already running;
handlers remain responsible for bounding their own work. A page can retain at most 128 requests.

`events` has typed listeners for everything Sabine sends: tray, global shortcut and second-launch
activations, renderer crashes, and guest lifecycle, navigation, download, shortcut,
wheel and favicon events.

Bytes travel as they are, without JSON or base64. Send them with a call, and receive them from a
handler that answers with `BridgeResponse::bytes` or from events sent with `emit_bytes`, as a
`Uint8Array`. Each message carries up to 32 MiB:

```js
await invoke("file.save", { path }, { body: new TextEncoder().encode(text) });
const contents = await invoke("file.read", { path }); // Uint8Array
listen(`terminal.output.${id}`, (bytes) => terminal.write(bytes));
```

## Typed bindings

Sabine writes a TypeScript client for the commands and events an app registers, so the page and
the app share one contract. Name the file in `Sabine.toml`:

```toml
[bindings]
path = "ui/src/bindings.ts"
```

Commands registered with `bridge_typed` or `bridge_typed_async`, and events declared with
`bridge_event`, end up in it. Their params and results derive `specta::Type` next to serde's
derives, and serde's attributes shape the TypeScript types:

```rust
use sabine::BridgeEvent;
use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Deserialize, Type)]
#[serde(rename_all = "camelCase")]
struct Save {
    path: String,
    keep_backup: Option<bool>,
}

#[derive(Serialize, Type)]
struct Saved {
    bytes: u64,
}

const SAVED: BridgeEvent<Saved> = BridgeEvent::new("notes.saved");

window
    .bridge_typed("notes.save", |save: Save| save_note(save))
    .bridge_event(&SAVED)
```

```ts
import { commands, events } from "./bindings";

const { bytes } = await commands.notesSave({ path, keepBackup: true }, { timeoutMs: 10000 });
events.notesSaved(({ bytes }) => console.log(bytes));
```

Command and event names become camelCase keys. Params with no fields leave the function without a
`params` argument. Each type reads as the app sends it, and params as the app accepts them, so a
field with a default may be left out. Numbers of every width are `number`, which is what the page
receives. `bridge_type::<T>()` adds a type that no command or event names, for a package of shared page code that works with it. The app sends declared events with `BridgeEventEmitter::send`, and bytes events, typed
`BridgeEvent<Bytes>`, with `send_bytes`.

`sabine dev` rewrites the file before it starts the page. `sabine bindings` writes it on its own,
and `sabine bindings --check` fails without writing when it is out of date, for CI. Both build and
run the app, which writes the file and exits before it opens a window, so check the file in with
the rest of the page.

## Window controls

```js
import { appWindow } from "@lantharos/sabine";

appWindow.show();
appWindow.hide();
appWindow.toggleMaximize();
appWindow.startDrag();
```

A settings page that records keyboard shortcuts can ask the desktop to hand over every key while
the window is focused, including combinations the desktop normally keeps for itself:

```js
await appWindow.inhibitShortcuts(true);
// record the shortcut
await appWindow.inhibitShortcuts(false);
```

This works on Wayland compositors that support keyboard shortcut inhibiting, on Windows, where
Alt+Tab, Alt+Esc, Ctrl+Esc and the Windows key reach the page, and on macOS, where Cmd+Tab,
Cmd+Space and every other combination with Command, Control or Option do. Some compositors ask the
user before they allow it, and macOS needs the app to have the Accessibility permission; until it
does, the call rejects and macOS offers to open the settings that grant it.

Wayland desktops only let a window take focus when it presents a token from the app the person is
using. Before handing something to another app, such as a link to open in the browser, ask for one
and pass it on with the request; the app process puts it in the launched app's environment as
`XDG_ACTIVATION_TOKEN`, or in a portal request as `activation_token`:

```js
const token = await appWindow.activationToken();
await invoke("open_link", { url, token });
```

The token is made from the window's keyboard focus, so it only brings the other window forward
while this one is still in front. It is `null` where the desktop needs none.

Pages can pause work while nobody can see them. `appWindow.visible` is false while the window is
hidden or the desktop reports it out of sight, for example when it is minimized or fully covered,
and `appWindow.suspended` is true while Sabine runs the page at its background frame rate:

```js
const stop = appWindow.onVisibilityChanged(({ visible, suspended }) => {
  if (visible) startSampling();
  else stopSampling();
});
```

While a shown window is out of sight, Chromium also stops rendering it and treats the document as
hidden. Rust code can follow the same changes with `SabineWindow::on_visibility_changed`.

Windows the desktop maximized, made fullscreen or tiled against a screen edge meet the edges of the
screen, so pages with rounded corners square them off while `appWindow.maximized`,
`appWindow.fullscreen` or `appWindow.tiled` is true. Sabine squares the window's regions itself:

```js
const stop = appWindow.onStateChanged(({ maximized, fullscreen, tiled }) => {
  document.documentElement.classList.toggle("docked", maximized || fullscreen || tiled);
});
```

Once a window has been out of sight for a while (five minutes for `.app()` windows), Sabine freezes
its page: no JavaScript runs and the page's caches are released, while everything it holds stays in
memory, so it continues instantly when the window comes back. The document receives the standard
`freeze` and `resume` events. Pages playing audio or video, in a call, or running an activity that
keeps them running are never frozen.

Glass windows can change which parts are blurred, opaque or clickable while they run, for example
to drop a sidebar's blur while it is hidden:

```js
import { appWindow, region } from "@lantharos/sabine";

await appWindow.setRegions({
  blur: region.empty(),
  opaque: region.contentAfterSidebarRoundedRight(0, 0, 12),
});
```

Rust code can do the same with `set_regions` or, for one window, `set_regions_of` on the app's
`BridgeEventEmitter`.

A window appears as soon as its page has loaded and painted. A page that still fetches what its
first view shows can keep the window off screen until that view is ready, so it never opens on an
empty frame. Call `revealWhen` while the page starts and settle the promise once the view is in the
document:

```js
appWindow.revealWhen(loadSettings().then(render));
```

The window then appears with the next frame the page paints. If the promise takes longer than five
seconds, the window shows the page anyway. Apps that enable Sabine's loading screen show it in the
meantime once the launch takes longer than its delay, and it fades into the page.

## Tray

An app that configures a tray icon can change it while it runs. A left click on the icon arrives
as `events.trayActivate` with `itemId: null`; the menu opens on a right click.

```js
import { events, tray } from "@lantharos/sabine";

await tray.update({
  tooltip: "Syncing",
  menu: [
    { id: "open", label: "Open" },
    { id: "pause", label: "Pause sync", type: "checkbox", checked: false },
    { id: "accounts", label: "Accounts", type: "submenu", items: [{ id: "work", label: "Work" }] },
    { type: "separator" },
    { id: "quit", label: "Quit" },
  ],
});

events.trayActivate(({ itemId, checked }) => console.log(itemId, checked));
events.globalShortcutFailed(({ id, message }) => console.warn(id, message));
```

## Notifications

`new Notification(title, { body, tag, silent })` shows a desktop notification from the app's own
pages, without asking for permission. Clicking it brings the window forward and fires its `click`
event. Sabine also reads `urgency` (`"low"`, `"normal"` or `"critical"`) and a freedesktop
`category` such as `"email.arrived"` from the options.

## Drag and drop

Dropped files carry their paths, and file URIs a page drags out reach other apps as files:

```js
zone.addEventListener("drop", (event) => {
  const paths = [...event.dataTransfer.files].map((file) => file.path);
});

item.addEventListener("dragstart", (event) => {
  event.dataTransfer.setData("text/uri-list", "file:///home/ana/Notes.txt");
});
```

A drop's `text/uri-list` lists the dropped files as `file://` URIs too. Only the app's own pages get
paths.

## Appearance

```js
import { events, system } from "@lantharos/sabine";

const { colorScheme, accentColor } = await system.appearance();
events.appearanceChanged(({ colorScheme, accentColor }) => applyTheme(colorScheme, accentColor));
```

The accent color is also set on the page as the `--sabine-accent-color` CSS property.

## Clipboard

Copy, cut, paste and `navigator.clipboard` work with the desktop clipboard as they do in a browser,
and on Linux selecting text sets the primary selection that middle-click pastes. For other formats,
such as a file manager's copied files, read and write any MIME type:

```js
import { clipboard } from "@lantharos/sabine";

await clipboard.write({
  "text/uri-list": "file:///home/me/notes.txt\r\n",
  "x-special/gnome-copied-files": "copy\nfile:///home/me/notes.txt",
});
const { "text/plain": text } = await clipboard.read({ types: ["text/plain"] });
const selected = await clipboard.read({ selection: "primary" });
```

Text types arrive as strings and other types as `Uint8Array`s. Every desktop carries any MIME
type, including files as `text/uri-list`, which Windows and macOS exchange as their own file lists.
The primary selection is Linux only.

## Printing

```js
import { print } from "@lantharos/sabine";

await print.page();
await print.element(document.querySelector("#receipt"));
const printed = await print.files(["/home/ana/Invoice.pdf", "/home/ana/Beach.jpg"]);
```

`print.page()` prints the page, like `window.print()`. `print.element()` prints one element on its
own, with images, canvases and videos scaled to fit the paper. `print.files()` prints PDF, image and
text files as one document, each image and text file on a sheet of its own. It resolves `false`
when the person cancels, and rejects with a message naming any file it can't print.

On Linux people choose the printer, pages, paper and orientation in the desktop's own print dialog,
through its print portal. Pages, images and text are laid out on the paper chosen there, and PDF
files go to the printer as they are. On macOS and Windows the system's print dialog opens, and
`print.files()` resolves once it does.

Rust code prints files from a window with `print_files` on the app's `BridgeEventEmitter`.

## Guests

```js
import { guest, Guest } from "@lantharos/sabine";

const tab = await guest.create({
  url: "https://example.com",
  bounds: { x: 16, y: 64, width: 900, height: 600 },
  partition: "persist:browser",
  interceptedShortcuts: ["Primary+T", "Primary+K"],
  interceptHorizontalWheel: true,
});

await tab.navigate("https://example.com/docs");
await tab.setBounds({ x: 16, y: 64, width: 1100, height: 700 });
await tab.destroy();

// Guest.create is also available directly:
const preview = await Guest.create({ html: "<h1>Hi</h1>", bounds: { x: 0, y: 0, width: 320, height: 200 } });
await preview.navigate({ html: "<h1>Updated</h1>" });

for (const info of await guest.list()) {
  console.log(info.id, info.url, info.loading);
}
```

`popupPolicy` decides what happens when a guest opens a window: `deny` (the default) emits
`guest.newWindow` for the page to handle, `allow` lets Chromium open a popup, `navigateSame` loads
the link in the same guest, and `openGuest` opens another guest in the same partition. Guest pages
have no bridge access unless created with `allowBridge: true`, and even then cannot manage guests.

## Activity and popups

```js
import { activity, popup } from "@lantharos/sabine";

const busy = await activity.begin({ name: "Indexing" });
try {
  // …
} finally {
  await busy.end();
}

await popup.open({ x: 40, y: 80, width: 280, height: 160, html: "<p>Menu</p>" });
await popup.close();
```

An activity keeps the page running while its window is out of sight, instead of letting Sabine
freeze it, unless it is started with `keepRunning: false`. `activity.list()` returns the running
activities and how many of them keep the page running. Rust code starts the same activities with
`begin_activity`.

## Opening URLs and documents

Declare URL schemes with `app.mime_types = ["x-scheme-handler/my-app"]` in `Sabine.toml`.
For Linux and Windows, use `.single_instance(SingleInstancePolicy::FocusExisting)` in Rust to
route later launches to the running app. macOS delivers URLs and documents through its application
delegate; test with the bundled application.

```js
import { app, events } from "@lantharos/sabine";

async function openPending() {
  for (const url of await app.takeOpenUrls()) {
    await openDocument(url);
  }
}

const unsubscribe = events.openUrlsAvailable(openPending);
await openPending();
```

`takeOpenUrls()` consumes pending URLs, including those received before the page loaded. Call
`unsubscribe()` when disposing the listener. URLs remain application input: validate their scheme
and contents before acting on them.

## Local files

Pages load from the app's own directory and cannot read other files by default. An app that needs
them, such as a file manager showing thumbnails, sets `local_files = true` under `[web]` in
`Sabine.toml` (or `.local_files(true)` in Rust), then builds URLs with `fileUrl`:

```js
import { fileUrl } from "@lantharos/sabine";

image.src = fileUrl("/home/me/Pictures/photo.png");
```

These URLs work only inside the app's own pages.

## Native video

Sabine's Chromium cannot decode H.264, HEVC or AAC. `NativeVideo` plays those on a native surface
beneath the page, with the page's own controls on top, on the platforms listed in the
[implementation guide](../../docs/implementation-guide.md#native-media). It mirrors the parts of
`HTMLVideoElement` a custom player uses, so a player can switch to it when a `<video>` cannot play
its source:

```js
import { NativeVideo, fileUrl } from "@lantharos/sabine";

video.addEventListener("error", async () => {
  if (!NativeVideo.isSupported()) return;
  const native = await NativeVideo.create(fileUrl(path), { element: stage, autoplay: true });
  native.addEventListener("timeupdate", () => render(native.currentTime, native.duration));
  native.addEventListener("error", () => showError(native.error.message));
});
```

The video fills `element`, follows its layout, scrolling and rounded corners, and letterboxes to
its aspect ratio. It shows through the page, so the window must be transparent, and
`NativeVideo.isSupported()` is false in opaque windows. The element and everything painted beneath
it must be transparent where the video is. When opaque content lies
beneath, such as a dialog over the rest of the app, pass `cutout: container` to clip the video's
shape out of that container; content inside it can then no longer draw over the video. Call
`updateRect()` while moving the element with a transform, and `destroy()` when the player goes
away.

It supports `play()`, `pause()`, `currentTime`, `fastSeek()`, `playbackRate`, `volume`, `muted`,
`loop`, `duration`, `paused`, `ended`, `videoWidth` and `videoHeight`, and dispatches the usual
media events. Tracks are listed in `audioTracks` and `subtitleTracks` and chosen with
`selectAudioTrack(id)` and `selectSubtitleTrack(id)`; `activeCue` holds the subtitle showing now
and changes with `cuechange`. On macOS a cue's `end` is `null`, since it shows until the next cue
replaces or clears it. Sources can be `http(s)` URLs, app files, or `fileUrl` paths in apps
with local file access.

## Availability

```js
import { isAvailable, sabine } from "@lantharos/sabine";

if (isAvailable()) {
  console.log(sabine().bridge.commands);
}
```

These helpers call into the bridge Sabine injects into your page. Use them from UI code that
runs inside a Sabine window.
