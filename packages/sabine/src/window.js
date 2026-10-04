import { listen, requireApi } from "./bridge.js";

/** @type {import("../types/window.js").AppWindow} */
export const appWindow = {
  show() {
    requireApi().window.show();
  },
  hide() {
    requireApi().window.hide();
  },
  focus(activationToken) {
    requireApi().window.focus(activationToken);
  },
  close() {
    requireApi().window.close();
  },
  minimize() {
    requireApi().window.minimize();
  },
  maximize() {
    requireApi().window.maximize();
  },
  toggleMaximize() {
    requireApi().window.toggleMaximize();
  },
  setFullscreen(enabled) {
    requireApi().window.setFullscreen(Boolean(enabled));
  },
  restore() {
    requireApi().window.restore();
  },
  startDrag() {
    requireApi().window.startDrag();
  },
  inhibitShortcuts(enabled) {
    return requireApi().window.inhibitShortcuts(enabled);
  },
  setRegions(regions) {
    return requireApi().window.setRegions(regions);
  },
  controlsOverlay() {
    return requireApi().window.controlsOverlay();
  },
  get visible() {
    return requireApi().window.visible;
  },
  get suspended() {
    return requireApi().window.suspended;
  },
  onVisibilityChanged(callback) {
    return listen("window.visibility", callback);
  },
};

/** @type {import("../types/window.js").SabinePopupApi} */
export const popup = {
  open(options = {}) {
    return requireApi().popup.open(options);
  },
  close() {
    return requireApi().popup.close();
  },
};
