export interface GuestBounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

/**
 * What a guest does with `window.open` and `target="_blank"` links: emit
 * `guest.newWindow` for the app (`deny`), let Chromium open a popup (`allow`),
 * navigate in place (`navigateSame`), or open another guest in the same
 * partition (`openGuest`).
 */
export type GuestPopupPolicy = "deny" | "allow" | "navigateSame" | "openGuest";

export interface GuestCreateOptions {
  id?: string;
  url?: string;
  html?: string;
  bounds: GuestBounds;
  partition?: string;
  allowBridge?: boolean;
  /** Accelerators consumed while the guest is focused (e.g. `Primary+K`). */
  interceptedShortcuts?: string[];
  /** Consume predominantly horizontal wheel/trackpad input over the guest. */
  interceptHorizontalWheel?: boolean;
  visible?: boolean;
  popupPolicy?: GuestPopupPolicy;
  allowDownloads?: boolean;
  /** `#RRGGBB` or `#AARRGGBB`. */
  backgroundColor?: string;
}

export interface GuestInfo {
  id: string;
  url: string;
  title: string;
  bounds: GuestBounds;
  visible: boolean;
  loading: boolean;
  canGoBack: boolean;
  canGoForward: boolean;
  partition: string;
  allowBridge: boolean;
  popupPolicy: GuestPopupPolicy;
  zoomFactor: number;
}

/** A URL, or an HTML document to show instead. */
export type GuestNavigateTarget = string | { url: string } | { html: string };

export interface GuestPreview {
  /** A PNG of the guest as it is drawn now. */
  dataUrl: string;
}

export interface GuestIdEvent {
  id: string;
}

export interface GuestLoadingEvent {
  id: string;
  loading: boolean;
}

export interface GuestTitleEvent {
  id: string;
  title: string;
}

export interface GuestNavigatedEvent {
  id: string;
  url: string;
  title: string;
  canGoBack: boolean;
  canGoForward: boolean;
}

export interface GuestNewWindowEvent {
  id: string;
  url: string;
  disposition: string;
}

export interface GuestDownloadEvent {
  guestId: string;
  downloadId: string;
  url: string;
  filename: string;
  mimeType: string;
  totalBytes: number;
  receivedBytes: number;
  state: "requested" | "progress" | "completed" | "cancelled" | "interrupted";
  savePath?: string | null;
  error?: string | null;
}

export type GuestDownloadAction = "accept" | "cancel" | "pause" | "resume";

export interface GuestDownloadOptions {
  savePath?: string;
  showDialog?: boolean;
}

export interface GuestShortcutEvent {
  id: string;
  accelerator: string;
  key: string;
  repeat: boolean;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
}

export interface GuestWheelEvent {
  id: string;
  deltaX: number;
  deltaY: number;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
}

export interface GuestFaviconEvent {
  id: string;
  favicons: string[];
}

interface GuestOperations {
  list(): Promise<GuestInfo[]>;
  get(id: string): Promise<GuestInfo>;
  destroy(id: string): Promise<GuestIdEvent>;
  navigate(id: string, target: GuestNavigateTarget): Promise<void>;
  setBounds(id: string, bounds: GuestBounds): Promise<GuestInfo>;
  setVisible(id: string, visible: boolean): Promise<GuestInfo>;
  /** Hides every guest while the page draws over them, such as a dialog. */
  setCovered(covered: boolean): Promise<void>;
  capturePreview(id: string): Promise<GuestPreview>;
  focus(id: string): Promise<void>;
  reload(id: string, options?: { ignoreCache?: boolean }): Promise<void>;
  goBack(id: string): Promise<void>;
  goForward(id: string): Promise<void>;
  setZoom(id: string, factor: number): Promise<GuestInfo>;
  executeJavaScript(id: string, code: string): Promise<void>;
  downloadAction(
    downloadId: string,
    action: GuestDownloadAction,
    options?: GuestDownloadOptions,
  ): Promise<void>;
}

export interface SabineGuestApi extends GuestOperations {
  create(options: GuestCreateOptions): Promise<GuestInfo>;
}

export interface GuestHelpers extends GuestOperations {
  create(options: GuestCreateOptions): Promise<Guest>;
}

export declare class Guest {
  readonly id: string;
  constructor(id: string);
  static create(options: GuestCreateOptions): Promise<Guest>;
  get(): Promise<GuestInfo>;
  navigate(target: GuestNavigateTarget): Promise<void>;
  setBounds(bounds: GuestBounds): Promise<GuestInfo>;
  setVisible(visible: boolean): Promise<GuestInfo>;
  focus(): Promise<void>;
  reload(options?: { ignoreCache?: boolean }): Promise<void>;
  goBack(): Promise<void>;
  goForward(): Promise<void>;
  setZoom(factor: number): Promise<GuestInfo>;
  executeJavaScript(code: string): Promise<void>;
  capturePreview(): Promise<GuestPreview>;
  destroy(): Promise<GuestIdEvent>;
}

export declare const guest: GuestHelpers;
