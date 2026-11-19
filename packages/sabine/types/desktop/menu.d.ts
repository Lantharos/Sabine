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
}

/** Where someone asked for a context menu, and what applies there. */
export interface MenuContext {
  /** Where to open the menu, in the CSS pixels of the app's page. */
  x: number;
  y: number;
  /** The guest asked over, or `null` for the app's own page. */
  guest: string | null;
  /** Whether the menu was asked for over text that can be edited. */
  editable: boolean;
  /** The selected text, or an empty string. */
  selection: string;
  /** The link asked over, or `null`. */
  link: string | null;
  /** The misspelled word asked over, which `spellcheck.replace` corrects. */
  misspelled: string | null;
  suggestions: string[];
  canUndo: boolean;
  canRedo: boolean;
  canCut: boolean;
  canCopy: boolean;
  canPaste: boolean;
  canSelectAll: boolean;
}

export interface SabineMenuApi {
  /**
   * Calls `callback` each time someone asks for a context menu, with the
   * mouse or the keyboard, in the app's page or a guest it shows. Pages draw
   * the menu themselves; without one, nothing opens.
   */
  onContext(callback: (context: MenuContext) => void): () => void;
  /**
   * Does what a role does, such as pasting into the focused field. Items that
   * edit should keep the field focused, by cancelling `mousedown`.
   */
  run(role: MenuRole): Promise<void>;
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
  /** Calls `callback` with each item chosen from the app menu bar. */
  onSelect(callback: (selection: MenuSelection) => void): () => void;
  onAppMenuChanged(callback: (items: MenuItem[]) => void): () => void;
}

/**
 * The page's own context menu, menus at a point of the page, and the app menu
 * bar, in the app's own pages.
 */
export declare const menu: SabineMenuApi;
