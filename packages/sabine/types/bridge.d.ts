import type { SabineActivityApi } from "./activity.js";
import type { SabineClipboardApi } from "./clipboard.js";
import type { SabineGuestApi } from "./guest.js";
import type { SystemAppearance } from "./system.js";
import type { SabinePopupApi, SabineWindowApi } from "./window.js";

export type JsonValue =
  | null
  | boolean
  | number
  | string
  | JsonValue[]
  | { [key: string]: JsonValue };

export interface InvokeOptions {
  signal?: AbortSignal;
  /** Deadline in milliseconds; defaults to 60000. */
  timeoutMs?: number;
  /** Bytes sent with the call, up to 32 MiB, which the handler reads as `command.body`. */
  body?: ArrayBuffer | ArrayBufferView | Blob;
}

export interface SabineBridge {
  readonly __native: true;
  readonly commands: string[];
  invoke(name: string, params?: Record<string, unknown>, options?: InvokeOptions): Promise<unknown>;
  listen(name: string, callback: (payload: unknown) => void): () => void;
}

/** The API Sabine injects into its pages as `window.sabine`. */
export interface SabineApi {
  bridge: SabineBridge;
  window: SabineWindowApi;
  guest: SabineGuestApi;
  activity: SabineActivityApi;
  popup: SabinePopupApi;
  system: { appearance(): Promise<SystemAppearance> };
  /** Present in the app's own pages. */
  clipboard?: SabineClipboardApi;
}

export declare function isAvailable(): boolean;
export declare function fileUrl(path: string): string;
export declare function sabine(): SabineApi;
export declare function invoke<T = unknown>(
  name: string,
  params?: Record<string, unknown>,
  options?: InvokeOptions,
): Promise<T>;
export declare function listen<T = unknown>(
  name: string,
  callback: (payload: T) => void,
): () => void;

export declare const bridge: {
  commands(): string[];
  invoke: typeof invoke;
  listen: typeof listen;
};

export declare const app: {
  /** Consumes pending URLs from initial launch and subsequent OS activations. */
  takeOpenUrls(): Promise<string[]>;
};
