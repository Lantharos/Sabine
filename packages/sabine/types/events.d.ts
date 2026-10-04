import type { SystemAppearance } from "./system.js";
import type {
  GuestDownloadEvent,
  GuestFaviconEvent,
  GuestIdEvent,
  GuestInfo,
  GuestLoadingEvent,
  GuestNavigatedEvent,
  GuestNewWindowEvent,
  GuestShortcutEvent,
  GuestTitleEvent,
  GuestWheelEvent,
} from "./guest.js";

export interface TrayActivateEvent {
  trayId: string;
  /** The menu item chosen, or `null` when the icon itself was activated. */
  itemId: string | null;
  action: string | null;
  /** A checkbox item's new state. */
  checked: boolean | null;
}

export interface GlobalShortcutEvent {
  id: string;
  action: string;
  /** Pass to `appWindow.focus` so the desktop lets the window take focus. */
  activationToken: string | null;
}

/** A global shortcut the desktop did not register, such as one another app holds. */
export interface GlobalShortcutFailedEvent {
  id: string;
  action: string;
  message: string;
}

export interface SingleInstanceEvent {
  policy: "allowMultiple" | "reuseExisting" | "focusExisting";
  /** The arguments the second launch received. */
  arguments: string[];
  workingDirectory: string | null;
  /** Pass to `appWindow.focus` so the desktop lets the window take focus. */
  activationToken: string | null;
}

export interface RendererCrashedEvent {
  /** The guest whose renderer stopped, or an empty string for the app's own page. */
  guestId: string;
  code: number;
  /** Whether Sabine is reloading the page, which it stops doing after three crashes in a minute. */
  recovering: boolean;
}

type Listen<T> = (callback: (payload: T) => void) => () => void;

export declare const events: {
  openUrlsAvailable: Listen<null>;
  trayActivate: Listen<TrayActivateEvent>;
  globalShortcut: Listen<GlobalShortcutEvent>;
  globalShortcutFailed: Listen<GlobalShortcutFailedEvent>;
  singleInstance: Listen<SingleInstanceEvent>;
  rendererCrashed: Listen<RendererCrashedEvent>;
  appearanceChanged: Listen<SystemAppearance>;
  guestCreated: Listen<GuestInfo>;
  guestDestroyed: Listen<GuestIdEvent>;
  guestLoading: Listen<GuestLoadingEvent>;
  guestTitle: Listen<GuestTitleEvent>;
  guestNavigated: Listen<GuestNavigatedEvent>;
  guestNewWindow: Listen<GuestNewWindowEvent>;
  guestDownload: Listen<GuestDownloadEvent>;
  guestShortcut: Listen<GuestShortcutEvent>;
  guestWheel: Listen<GuestWheelEvent>;
  guestFavicon: Listen<GuestFaviconEvent>;
};
