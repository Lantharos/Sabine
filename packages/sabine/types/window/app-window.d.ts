export interface SabineWindowApi {
  show(): void;
  hide(): void;
  focus(activationToken?: string): void;
  close(): void;
  /** Closes another of the app's windows, as its close button does. */
  close(id: number): Promise<void>;
  /**
   * Opens one of the app's own pages in another window, resolved against
   * this page's URL, and resolves with the window's id. It opens like the
   * app's first window, with its chrome and glass, as an ordinary window of
   * its own size. Rejects for pages of other sites.
   */
  open(url: string, options?: OpenWindowOptions): Promise<number>;
  minimize(): void;
  maximize(): void;
  unmaximize(): Promise<void>;
  toggleMaximize(): void;
  setFullscreen(enabled: boolean): void;
  restore(): void;
  startDrag(): void;
  /**
   * Names the window. The window otherwise follows the page's
   * `document.title`, and shows the app's title while the page has none; a
   * title set here wins over the page's until `setTitle(null)`.
   */
  setTitle(title: string | null): Promise<void>;
  /** Resizes the window, in CSS pixels. */
  setSize(width: number, height: number): Promise<void>;
  /** The smallest the window can get, in CSS pixels. `null` lifts the limit. */
  setMinSize(width: number, height: number): Promise<void>;
  setMinSize(limit: null): Promise<void>;
  /** The largest the window can get, in CSS pixels. `null` lifts the limit. */
  setMaxSize(width: number, height: number): Promise<void>;
  setMaxSize(limit: null): Promise<void>;
  /**
   * Moves the window's outer top left corner, in the physical pixels of
   * display bounds. Rejects on Wayland, where the desktop places windows.
   */
  setPosition(x: number, y: number): Promise<void>;
  /**
   * Keeps the window above other windows. On Wayland it rejects unless the
   * compositor offers `kestrel_window_v1`.
   */
  setAlwaysOnTop(enabled: boolean): Promise<void>;
  setResizable(enabled: boolean): Promise<void>;
  /**
   * While enabled and the window is focused, the desktop's own keyboard
   * shortcuts reach the page instead. Rejects on desktops that do not allow
   * it, and on macOS until the app has the Accessibility permission.
   */
  inhibitShortcuts(enabled: boolean): Promise<void>;
  /**
   * Replaces the window's blur, opaque and input regions, for example to drop
   * a sidebar's blur while the sidebar is hidden. A region left out returns to
   * its default: blur behind the whole window, nothing opaque, and input
   * everywhere.
   */
  setRegions(regions: WindowRegions): Promise<void>;
  /**
   * The top corners the system's own window controls cover, which an
   * app-drawn titlebar leaves free. With `AppChrome` on macOS the traffic
   * lights sit at the left; elsewhere the page draws its controls and nothing
   * is covered. The same values are set on the page as the
   * `--sabine-controls-left`, `--sabine-controls-right` and
   * `--sabine-controls-height` CSS properties.
   */
  controlsOverlay(): Promise<ControlsOverlay>;
  /**
   * A token that lets the window of another app take focus, for handing
   * something to that app because the person asked to, such as opening a
   * link in the browser. Pass it to the app being opened, for example as
   * `XDG_ACTIVATION_TOKEN` in its environment. It only works while this
   * window is in front, and is `null` where the desktop needs none.
   */
  activationToken(): Promise<string | null>;
  /**
   * What the window showed when the session it was open in ended, as it last
   * passed it to `keepState`, when the desktop opens the window again after
   * signing in. `null` for every other window, and on desktops that open apps
   * anew.
   */
  restoredState<T = unknown>(): Promise<T | null>;
  /**
   * Keeps what the window shows, such as the open document or page, for the
   * desktop to open the window again with after the session ends while it is
   * open. Call it whenever that changes; Sabine writes the latest state once
   * it stops changing for a moment, away from the page. Closing the window or
   * quitting the app forgets it, and `null` forgets it at once. Keep it small,
   * at most 256 KiB as JSON, and leave out anything secret: it is saved to the
   * app's state folder as it is.
   */
  keepState(state: unknown): void;
  /**
   * Keeps the window off screen until `ready` settles, so it first appears
   * with the page's first real view instead of an empty one. Call it while the
   * page starts, before it finishes loading, and settle `ready` once that view
   * is in the document. Slow pages show Sabine's loading screen in the
   * meantime, which fades into the page.
   */
  revealWhen(ready: PromiseLike<unknown>): void;
  /**
   * Whether people can see the window: it is shown and the desktop has not
   * reported it out of sight, as it does for minimized or fully covered
   * windows where it can.
   */
  readonly visible: boolean;
  /**
   * Whether Sabine is running the page at its background frame rate, for
   * example while the window is hidden or, with some lifecycle policies,
   * while it is not focused.
   */
  readonly suspended: boolean;
  /** Whether the desktop maximized the window. */
  readonly maximized: boolean;
  /** Whether the window covers its whole screen. */
  readonly fullscreen: boolean;
  /**
   * Whether the desktop tiled the window against a screen edge, such as
   * snapping it to one half of the screen. Wayland only.
   */
  readonly tiled: boolean;
  /** Calls `callback` whenever `visible` or `suspended` changes. */
  onVisibilityChanged(callback: (state: WindowVisibility) => void): () => void;
  /** Calls `callback` whenever `maximized`, `fullscreen` or `tiled` changes. */
  onStateChanged(callback: (state: WindowState) => void): () => void;
  /**
   * Calls `callback` whenever the corner the system's window controls cover
   * changes, such as when the window enters or leaves fullscreen.
   */
  onControlsOverlayChanged(callback: (overlay: ControlsOverlay) => void): () => void;
}

export interface OpenWindowOptions {
  /** The window's title while its page has no `document.title`. */
  title?: string;
  width?: number;
  height?: number;
  minWidth?: number;
  minHeight?: number;
  resizable?: boolean;
  alwaysOnTop?: boolean;
}

export interface ControlsOverlay {
  left: number;
  right: number;
  height: number;
}

export interface WindowVisibility {
  visible: boolean;
  suspended: boolean;
}

export interface WindowState {
  maximized: boolean;
  fullscreen: boolean;
  tiled: boolean;
}

/** A window region built with the `region` helpers. */
export interface WindowRegion {
  adaptive?: Record<string, unknown> | null;
  rects: { x: number; y: number; width: number; height: number }[];
}

export interface WindowRegions {
  blur?: WindowRegion | null;
  opaque?: WindowRegion | null;
  input?: WindowRegion | null;
}

export interface PopupOptions {
  x?: number;
  y?: number;
  width?: number;
  height?: number;
  html?: string;
  url?: string;
}

/**
 * One surface drawn over the window, such as a menu. It has no bridge access
 * and closes when the window is clicked outside it.
 */
export interface SabinePopupApi {
  open(options?: PopupOptions): Promise<void>;
  close(): Promise<void>;
}

export declare const appWindow: SabineWindowApi;
export declare const popup: SabinePopupApi;

/** Regions for `appWindow.setRegions`, sized with the window as it resizes. */
export declare const region: {
  empty(): WindowRegion;
  rect(x: number, y: number, width: number, height: number): WindowRegion;
  full(): WindowRegion;
  roundedRect(radius: number): WindowRegion;
  roundedLeft(width: number, radius: number): WindowRegion;
  titlebarAndSidebar(sidebarWidth: number, titlebarHeight: number, radius: number): WindowRegion;
  contentAfterSidebar(sidebarWidth: number, titlebarHeight?: number): WindowRegion;
  contentAfterSidebarRoundedRight(
    sidebarWidth: number,
    titlebarHeight: number,
    radius: number,
  ): WindowRegion;
};
