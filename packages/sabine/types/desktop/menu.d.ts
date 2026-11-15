export type MenuRole =
  | "about"
  | "hide"
  | "hideOthers"
  | "showAll"
  | "services"
  | "quit"
  | "undo"
  | "redo"
  | "cut"
  | "copy"
  | "paste"
  | "selectAll"
  | "minimize"
  | "zoom"
  | "fullscreen"
  | "close"
  | "window";

/** An item of a menu: a command, a separator, or a submenu of more items. */
export interface MenuItem {
  /** Reported when the item is chosen. Role items without one report their role. */
  id?: string;
  label?: string;
  /** Does what the role says instead of reporting the item. */
  role?: MenuRole;
  /**
   * Keys that choose the item while the page leaves them unhandled, such as
   * `Primary+Shift+S`. `Primary` is Command on macOS and Control elsewhere.
   */
  accelerator?: string;
  enabled?: boolean;
  /** Draws a check mark, on when `true`. */
  checked?: boolean;
  separator?: boolean;
  /** The items of the submenu this item opens. */
  items?: MenuItem[];
}

export interface MenuSelection {
  id: string;
  menu: "context" | "app";
}

export interface SabineMenuApi {
  /** Replaces the app's items in the window's context menu, after Chromium's. */
  setContextMenu(items: MenuItem[]): Promise<void>;
  /**
   * Shows a menu at a point of the page in CSS pixels, such as under a button
   * the page draws. Resolves with the id chosen, or `null` when it closes
   * without a choice.
   */
  popup(items: MenuItem[], at: { x: number; y: number }): Promise<string | null>;
  /** Replaces the app menu bar of every window. */
  setAppMenu(items: MenuItem[]): Promise<void>;
  /**
   * The app menu bar, with each item's label, accelerator and id filled in,
   * for pages that draw it themselves.
   */
  appMenu(): Promise<MenuItem[]>;
  /** Chooses an item of the app menu bar, as a page that draws it does. */
  activate(id: string): Promise<void>;
  /** Calls `callback` with each item chosen from the context menu or app menu bar. */
  onSelect(callback: (selection: MenuSelection) => void): () => void;
  onAppMenuChanged(callback: (items: MenuItem[]) => void): () => void;
}

/**
 * The app's items in the context menu, menus at a point of the page, and the
 * app menu bar, in the app's own pages.
 */
export declare const menu: SabineMenuApi;
