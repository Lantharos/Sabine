export interface TrayMenuItem {
  /** Reported as `itemId` when the item is chosen. */
  id?: string;
  label?: string;
  action?: string;
  enabled?: boolean;
  type?: "normal" | "separator" | "checkbox" | "submenu";
  /** The state of a `checkbox` item. */
  checked?: boolean;
  /** The items of a `submenu`. */
  items?: TrayMenuItem[];
}

/** Changes to the tray icon. Fields left out stay as they are. */
export interface TrayUpdate {
  /**
   * The name of one of the app's resources, the image's bytes, or `null` for
   * a plain dot.
   */
  icon?: string | ArrayBuffer | ArrayBufferView | Blob | null;
  /** Draw the icon as a macOS template image, tinted to match the menu bar. */
  template?: boolean;
  tooltip?: string | null;
  menu?: TrayMenuItem[];
}

export declare const tray: {
  /** Changes the app's tray icon. Rejects when the app has no tray icon. */
  update(changes: TrayUpdate): Promise<void>;
};
