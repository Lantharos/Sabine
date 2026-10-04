import { listen } from "./bridge.js";

/**
 * @param {string} name
 * @returns {(callback: (payload: any) => void) => () => void}
 */
const on = (name) => (callback) => listen(name, callback);

/** @type {typeof import("../types/events.js").events} */
export const events = {
  openUrlsAvailable: on("app.openUrlsAvailable"),
  trayActivate: on("tray.activate"),
  globalShortcut: on("globalShortcut.activate"),
  singleInstance: on("singleInstance.activate"),
  rendererCrashed: on("runtime.renderer-crashed"),
  guestCreated: on("guest.created"),
  guestDestroyed: on("guest.destroyed"),
  guestLoading: on("guest.loading"),
  guestTitle: on("guest.title"),
  guestNavigated: on("guest.navigated"),
  guestNewWindow: on("guest.newWindow"),
  guestDownload: on("guest.download"),
  guestShortcut: on("guest.shortcut"),
  guestWheel: on("guest.wheel"),
  guestFavicon: on("guest.favicon"),
};
