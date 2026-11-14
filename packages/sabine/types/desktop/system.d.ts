export interface SystemAppearance {
  colorScheme: "light" | "dark";
  /** The accent color people chose, as `#rrggbb`, where the desktop has one. */
  accentColor: string | null;
  /**
   * The material of the window's glass. On Kestrel, `"maris"` is the
   * desktop's surface colour over a wide blur of the wallpaper, which already
   * is the whole surface, so pages draw no tint of their own over it.
   * Everywhere else glass is `"luca"`.
   */
  backdrop: "luca" | "maris";
}

export declare const system: {
  /**
   * The desktop's light or dark preference and accent color. Pages also get
   * the accent color as the `--sabine-accent-color` CSS property, and hear
   * about changes through `events.appearanceChanged`.
   */
  appearance(): Promise<SystemAppearance>;
};
