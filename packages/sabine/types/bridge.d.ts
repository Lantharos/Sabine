import type { SabineActivityApi } from "./activity.js";
import type { SabineClipboardApi } from "./desktop/clipboard.js";
import type { SabineGuestApi } from "./guest.js";
import type { SabinePrintApi } from "./desktop/print.js";
import type { SabineDownloadsApi } from "./page/downloads.js";
import type { SabineSpellcheckApi } from "./page/spellcheck.js";
import type { SabineSystemApi } from "./desktop/system.js";
import type { SabinePopupApi, SabineWindowApi } from "./window/app-window.js";

export type JsonValue =
  | null
  | boolean
  | number
  | string
  | JsonValue[]
  | { [key: string]: JsonValue };

/** How long to wait for a command's answer. */
export interface CallOptions {
  signal?: AbortSignal;
  /** Deadline in milliseconds; defaults to 60000. */
  timeoutMs?: number;
}

export interface InvokeOptions extends CallOptions {
  /** Bytes sent with the call, up to 32 MiB, which the handler reads as `command.body`. */
  body?: ArrayBuffer | ArrayBufferView | Blob;
}

export interface SabineBridge {
  readonly __native: true;
  readonly commands: string[];
  invoke(name: string, params?: object, options?: InvokeOptions): Promise<unknown>;
  /**
   * Sends a command without waiting for its answer, for updates sent often
   * whose answers the page doesn't need, such as positions while dragging.
   */
  send(name: string, params?: object): void;
  listen(name: string, callback: (payload: unknown) => void): () => void;
}

/** The API Sabine injects into its pages as `window.sabine`. */
export interface SabineApi {
  bridge: SabineBridge;
  window: SabineWindowApi;
  guest: SabineGuestApi;
  activity: SabineActivityApi;
  popup: SabinePopupApi;
  system: SabineSystemApi;
  print: SabinePrintApi;
  downloads: SabineDownloadsApi;
  spellcheck: SabineSpellcheckApi;
  /** Present in the app's own pages. */
  clipboard?: SabineClipboardApi;
}

export declare function isAvailable(): boolean;
export declare function fileUrl(path: string): string;
export declare function sabine(): SabineApi;
export declare function invoke<T = unknown>(
  name: string,
  params?: object,
  options?: InvokeOptions,
): Promise<T>;
export declare function listen<T = unknown>(
  name: string,
  callback: (payload: T) => void,
): () => void;

export declare const bridge: {
  commands(): string[];
  invoke: typeof invoke;
  send: SabineBridge["send"];
  listen: typeof listen;
};

/**
 * A progress bar on the app's Dock or taskbar icon: a number from 0 to 1 for
 * work going normally, a state with its value, or `null` to hide it. Linux
 * docks draw paused and failed work like any other and show no
 * indeterminate bar.
 */
export type AppProgress =
  | number
  | null
  | { state: "normal" | "paused" | "error"; value: number }
  | { state: "indeterminate" };

export declare const app: {
  /** Consumes pending URLs from initial launch and subsequent OS activations. */
  takeOpenUrls(): Promise<string[]>;
  /**
   * Closes every window and ends the app, also one that runs in the
   * background or hides its window when it closes.
   */
  quit(): Promise<void>;
  /**
   * Turns the app's autostart entry on or off. Rejects when the app has no
   * autostart entry; development runs change nothing.
   */
  setAutostart(enabled: boolean): Promise<void>;
  /**
   * Shows a badge on the app's Dock or taskbar icon: a count, which hides at
   * zero, a few characters, or `null` for none. Linux docks show only counts,
   * so text rejects there.
   */
  setBadge(badge: number | string | null): Promise<void>;
  setProgress(progress: AppProgress): Promise<void>;
  /**
   * Adds a file the person opened to the desktop's list of recent files.
   * Rejects when the path is not a file.
   */
  addRecentDocument(path: string): Promise<void>;
  /** Takes the files the app added off the desktop's list of recent files. */
  clearRecentDocuments(): Promise<void>;
};
