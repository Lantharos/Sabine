/** @typedef {import("../../types/media/native-video.js").NativeVideoOptions} NativeVideoOptions */
/** @typedef {import("../../types/media/native-video.js").NativeVideoTrack} NativeVideoTrack */
/** @typedef {import("../../types/media/native-video.js").NativeVideoCue} NativeVideoCue */
/** @typedef {import("../../types/media/native-video.js").NativeVideoError} NativeVideoError */
/** @typedef {import("../../types/media/native-video.js").NativeVideoAttachOptions} NativeVideoAttachOptions */
/** @typedef {{ id: number, type: string, [field: string]: any }} MediaEvent */

import { LayoutTracker } from "./layout.js";

const EVENT = "sabine.media";
const CREATE = "sabine.media.create";

/** @type {Map<number, NativeVideo>} */
const players = new Map();
/** @type {Map<number, MediaEvent[]>} */
const early = new Map();
let creating = 0;
/** @type {(() => void) | null} */
let listening = null;

function bridge() {
  const bridge = globalThis.window?.sabine?.bridge;
  if (!bridge?.__native) {
    throw new Error("Native video only works inside a Sabine window.");
  }
  return bridge;
}

/** @param {MediaEvent} event */
function route(event) {
  const player = players.get(event.id);
  if (player) {
    player._receive(event);
    return;
  }
  if (creating === 0) return;
  const queued = early.get(event.id) ?? [];
  queued.push(event);
  early.set(event.id, queued);
}

/**
 * Plays video Chromium cannot decode on a native surface beneath the page.
 * It mirrors the parts of `HTMLVideoElement` a custom player uses, so pages
 * can switch to it when a `<video>` reports it cannot play a source.
 */
export class NativeVideo extends EventTarget {
  /** Whether this window can play native video. */
  static isSupported() {
    return Boolean(globalThis.window?.sabine?.bridge?.commands?.includes(CREATE));
  }

  /**
   * @param {string} src An `http(s)` URL, an app URL, or a `fileUrl` in apps with local file access.
   * @param {NativeVideoOptions} [options]
   * @returns {Promise<NativeVideo>}
   */
  static async create(src, options = {}) {
    const native = bridge();
    listening ??= native.listen(EVENT, (event) => route(/** @type {MediaEvent} */ (event)));
    const source = new URL(src, document.baseURI).href;
    creating += 1;
    /** @type {number} */
    let id;
    try {
      ({ id } = /** @type {{ id: number }} */ (
        await native.invoke(CREATE, {
          src: source,
          autoplay: Boolean(options.autoplay),
          loop: Boolean(options.loop),
          volume: options.volume ?? 1,
          muted: Boolean(options.muted),
          rate: options.playbackRate ?? 1,
        })
      ));
    } finally {
      creating -= 1;
    }
    const player = new NativeVideo(id, source, options);
    players.set(id, player);
    for (const event of early.get(id) ?? []) player._receive(event);
    early.delete(id);
    if (creating === 0) early.clear();
    if (options.element) player.attach(options.element, options);
    return player;
  }

  #id;
  #src;
  #time = 0;
  #timeAt = 0;
  #duration = NaN;
  #paused = true;
  #playing = false;
  #ended = false;
  #seeking = false;
  #readyState = 0;
  #rate = 1;
  #volume = 1;
  #muted = false;
  #loop = false;
  #width = 0;
  #height = 0;
  /** @type {NativeVideoError | null} */
  #error = null;
  /** @type {{ video: NativeVideoTrack[], audio: NativeVideoTrack[], subtitles: NativeVideoTrack[] }} */
  #tracks = { video: [], audio: [], subtitles: [] };
  /** @type {NativeVideoCue | null} */
  #cue = null;
  #cueTimer = 0;
  /** @type {LayoutTracker | null} */
  #layout = null;
  #rect = "";
  #destroyed = false;

  /**
   * @param {number} id
   * @param {string} src
   * @param {NativeVideoOptions} options
   */
  constructor(id, src, options) {
    super();
    this.#id = id;
    this.#src = src;
    this.#paused = !options.autoplay;
    this.#rate = options.playbackRate ?? 1;
    this.#volume = options.volume ?? 1;
    this.#muted = Boolean(options.muted);
    this.#loop = Boolean(options.loop);
  }

  get src() {
    return this.#src;
  }

  get currentTime() {
    if (!this.#playing) return this.#time;
    const elapsed = ((performance.now() - this.#timeAt) / 1000) * this.#rate;
    return Math.min(this.#time + elapsed, Number.isFinite(this.#duration) ? this.#duration : Infinity);
  }

  set currentTime(time) {
    this.#seek(time, false);
  }

  /** @param {number} time */
  fastSeek(time) {
    this.#seek(time, true);
  }

  get duration() {
    return this.#duration;
  }

  get paused() {
    return this.#paused;
  }

  get ended() {
    return this.#ended;
  }

  get seeking() {
    return this.#seeking;
  }

  get readyState() {
    return this.#readyState;
  }

  get error() {
    return this.#error;
  }

  get videoWidth() {
    return this.#width;
  }

  get videoHeight() {
    return this.#height;
  }

  get playbackRate() {
    return this.#rate;
  }

  set playbackRate(rate) {
    if (rate === this.#rate) return;
    this.#time = this.currentTime;
    this.#timeAt = performance.now();
    this.#rate = rate;
    this.#send("setRate", { rate });
    this.#emit("ratechange");
  }

  get volume() {
    return this.#volume;
  }

  set volume(volume) {
    if (volume === this.#volume) return;
    this.#volume = volume;
    this.#send("setVolume", { volume });
    this.#emit("volumechange");
  }

  get muted() {
    return this.#muted;
  }

  set muted(muted) {
    if (Boolean(muted) === this.#muted) return;
    this.#muted = Boolean(muted);
    this.#send("setMuted", { muted: this.#muted });
    this.#emit("volumechange");
  }

  get loop() {
    return this.#loop;
  }

  set loop(loop) {
    this.#loop = Boolean(loop);
    this.#send("setLoop", { loop: this.#loop });
  }

  /** @returns {NativeVideoTrack[]} */
  get audioTracks() {
    return this.#tracks.audio;
  }

  /** @returns {NativeVideoTrack[]} */
  get subtitleTracks() {
    return this.#tracks.subtitles;
  }

  /** The subtitle showing now, from the selected subtitle track. */
  get activeCue() {
    return this.#cue;
  }

  play() {
    if (this.#destroyed) return Promise.reject(new DOMException("The video was destroyed", "InvalidStateError"));
    if (this.#paused) {
      this.#paused = false;
      this.#emit("play");
    }
    if (this.#ended) {
      this.#ended = false;
      this.#time = 0;
    }
    return this.#request("play");
  }

  pause() {
    if (this.#paused) return;
    this.#paused = true;
    this.#stopClock();
    this.#send("pause");
    this.#emit("pause");
  }

  /** @param {string} id */
  selectAudioTrack(id) {
    return this.#request("selectTracks", { audio: id });
  }

  /** @param {string | null} id `null` turns subtitles off. */
  selectSubtitleTrack(id) {
    if (id === null) this.#showCue(null);
    return this.#request("selectTracks", { subtitle: id });
  }

  /**
   * Draws the video under `element`, following its layout, scrolling and
   * rounded corners. The element and everything drawn beneath it must be
   * transparent where the video shows; pass `cutout` to clip the video's shape
   * out of an ancestor instead, when nothing inside it draws over the video.
   * @param {Element} element
   * @param {NativeVideoAttachOptions} [options]
   */
  attach(element, options = {}) {
    this.detach();
    this.#layout = new LayoutTracker(element, options.cutout ?? null, (placement) =>
      this.#place(placement),
    );
  }

  detach() {
    if (!this.#layout) return;
    this.#layout.stop();
    this.#layout = null;
    this.#place({ x: 0, y: 0, width: 0, height: 0, visible: false });
  }

  /** Re-reads the attached element's position, e.g. during a transform animation. */
  updateRect() {
    this.#layout?.update();
  }

  destroy() {
    if (this.#destroyed) return;
    this.#destroyed = true;
    this.detach();
    this.#stopClock();
    clearTimeout(this.#cueTimer);
    players.delete(this.#id);
    this.#send("destroy");
  }

  /** @param {MediaEvent} event */
  _receive(event) {
    switch (event.type) {
      case "loaded":
        this.#readyState = 4;
        this.#setTime(event.time);
        if (event.duration != null) this.#duration = event.duration;
        for (const name of ["durationchange", "loadedmetadata", "loadeddata", "canplay", "canplaythrough"]) {
          this.#emit(name);
        }
        break;
      case "duration":
        this.#duration = event.duration;
        this.#emit("durationchange");
        break;
      case "size":
        this.#width = Math.round(event.width);
        this.#height = Math.round(event.height);
        this.#emit("resize");
        break;
      case "time":
        this.#setTime(event.time);
        this.#emit("timeupdate");
        break;
      case "state":
        this.#state(event.state);
        break;
      case "seeked":
        this.#seeking = false;
        this.#setTime(event.time);
        this.#emit("seeked");
        this.#emit("timeupdate");
        break;
      case "tracks":
        this.#tracks = { video: event.video, audio: event.audio, subtitles: event.subtitles };
        this.#emit("trackschange");
        break;
      case "cue":
        this.#showCue(event.text ? { text: event.text, start: event.start, end: event.end } : null);
        break;
      case "error":
        this.#error = { code: 4, message: event.message };
        this.#emit("error");
        break;
    }
  }

  /** @param {string} state */
  #state(state) {
    if (state === "playing") {
      this.#playing = true;
      this.#timeAt = performance.now();
      if (this.#cue) this.#expireCue(this.#cue);
      if (this.#paused) {
        this.#paused = false;
        this.#emit("play");
      }
      this.#emit("playing");
    } else if (state === "paused") {
      this.#stopClock();
      if (!this.#paused) {
        this.#paused = true;
        this.#emit("pause");
      }
    } else if (state === "buffering") {
      this.#stopClock();
      this.#emit("waiting");
    } else if (state === "ended") {
      this.#stopClock();
      this.#ended = true;
      if (!this.#paused) {
        this.#paused = true;
        this.#emit("pause");
      }
      this.#emit("ended");
    }
  }

  /**
   * @param {number} time
   * @param {boolean} fast
   */
  #seek(time, fast) {
    const target = Math.max(0, Number(time) || 0);
    this.#ended = false;
    this.#seeking = true;
    this.#setTime(target);
    this.#showCue(null);
    this.#send("seek", { time: target, fast });
    this.#emit("seeking");
    this.#emit("timeupdate");
  }

  /** @param {number} time */
  #setTime(time) {
    this.#time = time;
    this.#timeAt = performance.now();
  }

  #stopClock() {
    this.#time = this.currentTime;
    this.#playing = false;
  }

  /** @param {NativeVideoCue | null} cue */
  #showCue(cue) {
    clearTimeout(this.#cueTimer);
    if (cue === this.#cue) return;
    this.#cue = cue;
    this.#emit("cuechange");
    if (cue) this.#expireCue(cue);
  }

  /** @param {NativeVideoCue} cue */
  #expireCue(cue) {
    clearTimeout(this.#cueTimer);
    const end = cue.end;
    if (!this.#playing || end === null) return;
    const remaining = (end - this.currentTime) / this.#rate;
    this.#cueTimer = setTimeout(() => {
      if (this.#cue !== cue) return;
      if (this.currentTime >= end) this.#showCue(null);
      else this.#expireCue(cue);
    }, Math.max(16, remaining * 1000));
  }

  /** @param {Record<string, unknown>} rect */
  #place(rect) {
    const key = JSON.stringify(rect);
    if (key === this.#rect || this.#destroyed) return;
    this.#rect = key;
    this.#send("setRect", rect);
  }

  /**
   * @param {string} name
   * @param {object} [params]
   */
  async #request(name, params = {}) {
    if (this.#destroyed) return;
    await bridge().invoke(`sabine.media.${name}`, { id: this.#id, ...params });
  }

  /**
   * @param {string} name
   * @param {object} [params]
   */
  #send(name, params = {}) {
    if (this.#destroyed && name !== "destroy") return;
    bridge().send(`sabine.media.${name}`, { id: this.#id, ...params });
  }

  /** @param {string} name */
  #emit(name) {
    this.dispatchEvent(new Event(name));
  }
}
