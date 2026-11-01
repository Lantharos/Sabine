import type { activity } from "./types/activity.js";
import type {
  app,
  bridge,
  fileUrl,
  invoke,
  isAvailable,
  listen,
  SabineApi,
  sabine,
} from "./types/bridge.js";
import type { clipboard } from "./types/desktop/clipboard.js";
import type { events } from "./types/events.js";
import type { Guest, guest } from "./types/guest.js";
import type { NativeVideo } from "./types/media/native-video.js";
import type { print } from "./types/desktop/print.js";
import type { shell } from "./types/desktop/shell.js";
import type { system } from "./types/desktop/system.js";
import type { tray } from "./types/desktop/tray.js";
import type { appWindow, popup, region } from "./types/window/app-window.js";

export * from "./types/activity.js";
export * from "./types/bridge.js";
export * from "./types/desktop/clipboard.js";
export * from "./types/events.js";
export * from "./types/guest.js";
export * from "./types/media/native-video.js";
export * from "./types/desktop/print.js";
export * from "./types/desktop/shell.js";
export * from "./types/desktop/system.js";
export * from "./types/desktop/tray.js";
export * from "./types/window/app-window.js";

declare global {
  interface Window {
    sabine?: SabineApi;
  }
}

declare const api: {
  isAvailable: typeof isAvailable;
  fileUrl: typeof fileUrl;
  sabine: typeof sabine;
  invoke: typeof invoke;
  listen: typeof listen;
  bridge: typeof bridge;
  events: typeof events;
  app: typeof app;
  appWindow: typeof appWindow;
  Guest: typeof Guest;
  guest: typeof guest;
  activity: typeof activity;
  popup: typeof popup;
  clipboard: typeof clipboard;
  print: typeof print;
  region: typeof region;
  NativeVideo: typeof NativeVideo;
  shell: typeof shell;
  system: typeof system;
  tray: typeof tray;
};

export default api;
