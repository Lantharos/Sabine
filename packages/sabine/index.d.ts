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
import type { clipboard } from "./types/clipboard.js";
import type { events } from "./types/events.js";
import type { Guest, guest } from "./types/guest.js";
import type { NativeVideo } from "./types/media.js";
import type { tray } from "./types/tray.js";
import type { appWindow, popup, region } from "./types/window.js";

export * from "./types/activity.js";
export * from "./types/bridge.js";
export * from "./types/clipboard.js";
export * from "./types/events.js";
export * from "./types/guest.js";
export * from "./types/media.js";
export * from "./types/tray.js";
export * from "./types/window.js";

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
  region: typeof region;
  NativeVideo: typeof NativeVideo;
  tray: typeof tray;
};

export default api;
