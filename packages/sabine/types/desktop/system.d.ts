export interface SystemAppearance {
  colorScheme: "light" | "dark";
  /** The accent color people chose, as `#rrggbb`, where the desktop has one. */
  accentColor: string | null;
  /**
   * The material of the window's glass. On Kestrel, `"maris"` is the
   * desktop's surface colour over a wide blur of the wallpaper and `"niko"` a
   * blur of what is behind the window in the lightness of the surface colour.
   * Both already are the whole surface, so pages draw no tint of their own
   * over them. Everywhere else glass is `"luca"`.
   */
  backdrop: "luca" | "maris" | "niko";
}

/** A rectangle of the desktop in physical pixels. */
export interface DisplayRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface Display {
  /** Tells displays apart while they stay connected. */
  id: number;
  name: string | null;
  /** Where the display sits on the desktop, in the coordinates of `appWindow.setPosition`. */
  bounds: DisplayRect;
  /**
   * The part of the display windows open in, without the taskbar, dock or
   * menu bar. `null` on Wayland, which doesn't tell apps.
   */
  workArea: DisplayRect | null;
  scaleFactor: number;
  /** In hertz, where the desktop tells. */
  refreshRate: number | null;
  /** The display the desktop puts its taskbar or menu bar on. Never on Wayland. */
  primary: boolean;
}

export interface PowerSource {
  onBattery: boolean;
  /** The battery's charge in percent, on machines with a battery. */
  batteryLevel: number | null;
}

export interface PowerState {
  /** `null` until the desktop has reported it, shortly after the app starts. */
  source: PowerSource | null;
  locked: boolean;
}

export type IdleState = "idle" | "active";

export interface SabineSystemApi {
  /**
   * The desktop's light or dark preference, accent color and glass. Pages
   * also get the accent color as the `--sabine-accent-color` CSS property,
   * and hear about changes through `events.appearanceChanged`.
   */
  appearance(): Promise<SystemAppearance>;
  /**
   * The displays connected to the computer. Pages hear about changes through
   * `events.displaysChanged`.
   */
  displays(): Promise<Display[]>;
  /**
   * The power source and screen lock as the desktop last reported them.
   * Changes arrive through `events.powerSourceChanged`, `events.screenLocked`
   * and `events.screenUnlocked`.
   */
  power(): Promise<PowerState>;
  /**
   * Calls `callback` with `"idle"` once nobody used the keyboard, pointer or
   * touch for `seconds`, and with `"active"` when someone does again. Resolves
   * with a function that stops watching; the watch also stops when the page
   * goes away.
   */
  watchIdle(seconds: number, callback: (state: IdleState) => void): Promise<() => void>;
}

export declare const system: SabineSystemApi;
