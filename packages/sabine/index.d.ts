import type { SabineApi } from "./types/bridge.js";

export * from "./types/activity.js";
export * from "./types/bridge.js";
export * from "./types/desktop/clipboard.js";
export * from "./types/events.js";
export * from "./types/guest.js";
export * from "./types/media/native-video.js";
export * from "./types/desktop/print.js";
export * from "./types/desktop/shell.js";
export * from "./types/desktop/system.js";
export * from "./types/storage/secrets.js";
export * from "./types/storage/store.js";
export * from "./types/desktop/tray.js";
export * from "./types/window/app-window.js";

declare global {
  interface Window {
    sabine?: SabineApi;
  }
}
