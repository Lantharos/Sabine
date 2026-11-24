// `window.sabine.window`, `window.sabine.system` and `window.sabine.popup`,
// and the CSS properties that follow the window's controls and the desktop's
// accent color.

const windowCommand = function (action, value) {
  postNative("window", action, value == null ? "" : String(value));
};

const controlWindow = async function (control) {
  await window.sabine.bridge.invoke("sabine.window.control", control);
};
const pixels = (value) => Math.round(Number(value));
const windowSize = (width, height) => width == null ? null : { width: pixels(width), height: pixels(height) };

let windowState = { visible: true, suspended: false };
window.__sabineWindowStateSet = function (visible, suspended) {
  if (windowState.visible === visible && windowState.suspended === suspended) return;
  windowState = { visible, suspended };
  window.__sabineBridgeEmit("window.visibility", { visible, suspended });
};

let toplevelState = { maximized: false, fullscreen: false, tiled: false };
window.__sabineToplevelSet = function (maximized, fullscreen, tiled) {
  const state = toplevelState;
  if (state.maximized === maximized && state.fullscreen === fullscreen && state.tiled === tiled) return;
  toplevelState = { maximized, fullscreen, tiled };
  window.__sabineBridgeEmit("window.state", { ...toplevelState });
};

window.sabine.window = Object.assign(window.sabine.window || {}, {
  show() { windowCommand("show"); },
  hide() { windowCommand("hide"); },
  focus(activationToken) { windowCommand("focus", activationToken); },
  close(id) {
    if (id == null) return windowCommand("close");
    return window.sabine.bridge.invoke("app.closeWindow", { id }).then(() => {});
  },
  overlaysSupported() {
    return window.sabine.bridge.invoke("app.overlaysSupported");
  },
  open(url, options = {}) {
    return window.sabine.bridge.invoke("app.openWindow", { ...options, url: String(url) });
  },
  minimize() { windowCommand("minimize"); },
  maximize() { windowCommand("maximize"); },
  unmaximize() { return controlWindow({ op: "unmaximize" }); },
  toggleMaximize() { windowCommand("toggle-maximize"); },
  setFullscreen(enabled) { windowCommand(enabled ? "fullscreen" : "exit-fullscreen"); },
  restore() { windowCommand("restore"); },
  startDrag() { windowCommand("start-drag"); },
  setTitle(title) { return controlWindow({ op: "setTitle", title: title == null ? null : String(title) }); },
  setSize(width, height) { return controlWindow({ op: "setSize", width: pixels(width), height: pixels(height) }); },
  setMinSize(width, height) { return controlWindow({ op: "setMinSize", size: windowSize(width, height) }); },
  setMaxSize(width, height) { return controlWindow({ op: "setMaxSize", size: windowSize(width, height) }); },
  setPosition(x, y) { return controlWindow({ op: "setPosition", x: pixels(x), y: pixels(y) }); },
  setAlwaysOnTop(enabled) { return controlWindow({ op: "setAlwaysOnTop", enabled: Boolean(enabled) }); },
  setResizable(enabled) { return controlWindow({ op: "setResizable", enabled: Boolean(enabled) }); },
  requestAttention(critical = false) { return controlWindow({ op: "requestAttention", critical: Boolean(critical) }); },
  async inhibitShortcuts(enabled) {
    await window.sabine.bridge.invoke("sabine.window.inhibitShortcuts", { enabled: Boolean(enabled) });
  },
  async setRegions(regions) {
    await window.sabine.bridge.invoke("sabine.window.setRegions", regions);
  },
  async setControlRegions(regions) {
    await window.sabine.bridge.invoke(
      "sabine.window.setControlRegions",
      regions.map(({ action, x, y, width, height }) => ({ action, rect: { x, y, width, height } })),
    );
  },
  controlsOverlay() {
    return window.sabine.bridge.invoke("sabine.window.controlsOverlay");
  },
  activationToken() {
    return window.sabine.bridge.invoke("sabine.window.activationToken");
  },
  identifier() {
    return window.sabine.bridge.invoke("sabine.window.identifier");
  },
  restoredState() {
    return window.sabine.bridge.invoke("sabine.window.restoredState");
  },
  keepState(state) {
    window.sabine.bridge.invoke("sabine.window.keepState", { state: state ?? null }).catch(() => {});
  },
  revealWhen(ready) {
    const reveal = () => requestAnimationFrame(() => {
      window.sabine.bridge.invoke("sabine.window.reveal").catch(() => {});
    });
    window.sabine.bridge.invoke("sabine.window.holdReveal").catch(() => {});
    Promise.resolve(ready).then(reveal, reveal);
  },
  onVisibilityChanged(callback) { return window.sabine.bridge.listen("window.visibility", callback); },
  onStateChanged(callback) { return window.sabine.bridge.listen("window.state", callback); },
  onControlsOverlayChanged(callback) { return window.sabine.bridge.listen("window.controlsOverlay", callback); },
});
Object.defineProperties(window.sabine.window, {
  visible: { get: () => windowState.visible, configurable: true },
  suspended: { get: () => windowState.suspended, configurable: true },
  maximized: { get: () => toplevelState.maximized, configurable: true },
  fullscreen: { get: () => toplevelState.fullscreen, configurable: true },
  tiled: { get: () => toplevelState.tiled, configurable: true },
});

const showControlsOverlay = (overlay) => {
  const style = document.documentElement.style;
  for (const side of ["left", "right", "height"]) {
    style.setProperty("--sabine-controls-" + side, overlay[side] + "px");
  }
};
if (commands.has("sabine.window.controlsOverlay")) {
  window.sabine.bridge.listen("window.controlsOverlay", showControlsOverlay);
  window.sabine.window.controlsOverlay().then(showControlsOverlay, () => {});
}

window.sabine.system = {
  appearance() { return window.sabine.bridge.invoke("sabine.system.appearance"); },
  displays() { return window.sabine.bridge.invoke("sabine.system.displays"); },
};
const showAccent = (appearance) => {
  const style = document.documentElement.style;
  if (appearance?.accentColor) style.setProperty("--sabine-accent-color", appearance.accentColor);
  else style.removeProperty("--sabine-accent-color");
};
if (commands.has("sabine.system.appearance")) {
  window.sabine.bridge.listen("system.appearance", showAccent);
  window.sabine.system.appearance().then(showAccent, () => {});
}

window.sabine.popup = {
  open(options = {}) {
    return window.sabine.bridge.invoke("sabine.popup.open", {
      bounds: bounds(options),
      html: String(options.html || ""),
      url: String(options.url || ""),
    });
  },
  close() {
    return window.sabine.bridge.invoke("sabine.popup.close");
  },
};
