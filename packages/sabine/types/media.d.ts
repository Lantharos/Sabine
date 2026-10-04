export interface NativeVideoAttachOptions {
  /**
   * Ancestor to clip the video's shape out of, for opaque content beneath the
   * element. Nothing inside it can then draw over the video.
   */
  cutout?: Element;
}

export interface NativeVideoOptions extends NativeVideoAttachOptions {
  /** Element whose box the video fills; see `NativeVideo.attach`. */
  element?: Element;
  autoplay?: boolean;
  loop?: boolean;
  volume?: number;
  muted?: boolean;
  playbackRate?: number;
}

export interface NativeVideoTrack {
  id: string;
  language: string | null;
  label: string | null;
  codec: string | null;
  selected: boolean;
}

export interface NativeVideoCue {
  text: string;
  start: number;
  end: number;
}

export interface NativeVideoError {
  code: number;
  message: string;
}

export type NativeVideoEvent =
  | "loadedmetadata"
  | "loadeddata"
  | "canplay"
  | "canplaythrough"
  | "durationchange"
  | "timeupdate"
  | "play"
  | "playing"
  | "pause"
  | "waiting"
  | "seeking"
  | "seeked"
  | "ended"
  | "error"
  | "ratechange"
  | "volumechange"
  | "resize"
  | "trackschange"
  | "cuechange";

/**
 * Plays video Chromium cannot decode on a native surface beneath the page, with the
 * parts of `HTMLVideoElement` a custom player uses.
 */
export declare class NativeVideo extends EventTarget {
  static isSupported(): boolean;
  static create(src: string, options?: NativeVideoOptions): Promise<NativeVideo>;
  readonly src: string;
  currentTime: number;
  readonly duration: number;
  readonly paused: boolean;
  readonly ended: boolean;
  readonly seeking: boolean;
  readonly readyState: number;
  readonly error: NativeVideoError | null;
  readonly videoWidth: number;
  readonly videoHeight: number;
  playbackRate: number;
  volume: number;
  muted: boolean;
  loop: boolean;
  readonly audioTracks: NativeVideoTrack[];
  readonly subtitleTracks: NativeVideoTrack[];
  readonly activeCue: NativeVideoCue | null;
  fastSeek(time: number): void;
  play(): Promise<void>;
  pause(): void;
  selectAudioTrack(id: string): Promise<void>;
  selectSubtitleTrack(id: string | null): Promise<void>;
  attach(element: Element, options?: NativeVideoAttachOptions): void;
  detach(): void;
  updateRect(): void;
  destroy(): void;
  addEventListener(
    type: NativeVideoEvent,
    listener: (event: Event) => void,
    options?: boolean | AddEventListenerOptions,
  ): void;
  removeEventListener(
    type: NativeVideoEvent,
    listener: (event: Event) => void,
    options?: boolean | EventListenerOptions,
  ): void;
}
