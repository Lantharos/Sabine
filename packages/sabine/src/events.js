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
  globalShortcutFailed: on("globalShortcut.failed"),
  singleInstance: on("singleInstance.activate"),
  rendererCrashed: on("runtime.renderer-crashed"),
  appearanceChanged: on("system.appearance"),
  displaysChanged: on("system.displays"),
  storeChanged: on("store.changed"),
  pen: on("pen.state"),
  notificationClick: on("notification.click"),
  notificationClose: on("notification.close"),
  notificationError: on("notification.error"),
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
