# `@lantharos/sabine`

Typed helpers for pages running inside a Sabine window. Prefer these exports over reaching for
`window.sabine` directly.

## Install

```sh
bun add github:Lantharos/Sabine#v0.32
```

## Bridge commands

```js
import { invoke, listen, events, appWindow } from "@lantharos/sabine";

const { version } = await invoke("app.version");
listen("tray.click", () => appWindow.show());

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

events.fileDrag((event) => {
  console.log(event.phase, event.paths, event.x, event.y, event.action);
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

## Window controls

```js
import { appWindow } from "@lantharos/sabine";

appWindow.show();
appWindow.hide();
appWindow.toggleMaximize();
appWindow.startDrag();
```

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
```

## Activity and popups

```js
import { activity, popup } from "@lantharos/sabine";

const busy = await activity.begin({ label: "Indexing" });
try {
  // …
} finally {
  await busy.end();
}

await popup.open({ x: 40, y: 80, width: 280, height: 160, html: "<p>Menu</p>" });
await popup.close();
```

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

Sabine's Chromium cannot decode H.264, HEVC or AAC. On Linux, `NativeVideo` plays those on a native
surface beneath the page, with the page's own controls on top. It mirrors the parts of
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
its aspect ratio. It shows through the page, so the window must be transparent, and the element and
everything painted beneath it must be transparent where the video is. When opaque content lies
beneath, such as a dialog over the rest of the app, pass `cutout: container` to clip the video's
shape out of that container; content inside it can then no longer draw over the video. Call
`updateRect()` while moving the element with a transform, and `destroy()` when the player goes
away.

It supports `play()`, `pause()`, `currentTime`, `fastSeek()`, `playbackRate`, `volume`, `muted`,
`loop`, `duration`, `paused`, `ended`, `videoWidth` and `videoHeight`, and dispatches the usual
media events. Tracks are listed in `audioTracks` and `subtitleTracks` and chosen with
`selectAudioTrack(id)` and `selectSubtitleTrack(id)`; `activeCue` holds the subtitle showing now
and changes with `cuechange`. Sources can be `http(s)` URLs, app files, or `fileUrl` paths in apps
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
