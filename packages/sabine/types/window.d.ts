export interface SabineWindowApi {
  show(): void;
  hide(): void;
  focus(activationToken?: string): void;
  close(): void;
  minimize(): void;
  maximize(): void;
  toggleMaximize(): void;
  setFullscreen(enabled: boolean): void;
  restore(): void;
  startDrag(): void;
  /**
   * While enabled and the window is focused, the desktop's own keyboard
   * shortcuts reach the page instead. Rejects on macOS and on desktops that
   * do not allow it.
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
}

export interface WindowVisibility {
  visible: boolean;
  suspended: boolean;
}

export interface AppWindow extends SabineWindowApi {
  /** Calls `callback` whenever `visible` or `suspended` changes. */
  onVisibilityChanged(callback: (state: WindowVisibility) => void): () => void;
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

export declare const appWindow: AppWindow;
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
