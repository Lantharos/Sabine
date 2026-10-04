export interface SystemAppearance {
  colorScheme: "light" | "dark";
  /** The accent color people chose, as `#rrggbb`, where the desktop has one. */
  accentColor: string | null;
}

export declare const system: {
  /**
   * The desktop's light or dark preference and accent color. Pages also get
   * the accent color as the `--sabine-accent-color` CSS property, and hear
   * about changes through `events.appearanceChanged`.
   */
  appearance(): Promise<SystemAppearance>;
};
