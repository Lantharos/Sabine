// `window.sabine.window`, `window.sabine.system` and `window.sabine.popup`,
// and the CSS properties that follow the window's controls and the desktop's
// accent color.

const windowCommand = function (action, value) {
  postNative("window", action, value == null ? "" : String(value));
};

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
  close() { windowCommand("close"); },
  minimize() { windowCommand("minimize"); },
  maximize() { windowCommand("maximize"); },
  toggleMaximize() { windowCommand("toggle-maximize"); },
  setFullscreen(enabled) { windowCommand(enabled ? "fullscreen" : "exit-fullscreen"); },
  restore() { windowCommand("restore"); },
  startDrag() { windowCommand("start-drag"); },
  async inhibitShortcuts(enabled) {
    await window.sabine.bridge.invoke("sabine.window.inhibitShortcuts", { enabled: Boolean(enabled) });
  },
  async setRegions(regions) {
    await window.sabine.bridge.invoke("sabine.window.setRegions", regions);
  },
  controlsOverlay() {
    return window.sabine.bridge.invoke("sabine.window.controlsOverlay");
  },
  activationToken() {
    return window.sabine.bridge.invoke("sabine.window.activationToken");
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
