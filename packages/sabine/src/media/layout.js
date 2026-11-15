/** @typedef {import("./box.js").Box} Box */
/** @typedef {Box & { radius: number, clip: Box | null, visible: boolean }} Placement */

import { intersect, sameBox } from "./box.js";
import { observeMove } from "./move-observer.js";

const LAYOUT_EVENTS = ["scroll", "resize", "fullscreenchange", "transitionend", "animationend"];
const STYLE_ATTRIBUTES = ["style", "class"];

/** @type {Placement} */
export const HIDDEN = { x: 0, y: 0, width: 0, height: 0, radius: 0, clip: null, visible: false };

/**
 * @param {Placement} a
 * @param {Placement} b
 */
export function samePlacement(a, b) {
  return sameBox(a, b) && a.radius === b.radius && a.visible === b.visible && sameBox(a.clip, b.clip);
}

/** @param {Element} element */
function clipsOverflow(element) {
  const style = getComputedStyle(element);
  return style.overflowX !== "visible" || style.overflowY !== "visible";
}

/**
 * @param {Element} container
 * @param {Box} hole
 * @param {number} radius
 */
function holePath(container, hole, radius) {
  const box = container.getBoundingClientRect();
  const x = hole.x - box.x;
  const y = hole.y - box.y;
  const right = x + hole.width;
  const bottom = y + hole.height;
  const r = Math.min(radius, hole.width / 2, hole.height / 2);
  /**
   * @param {number} toX
   * @param {number} toY
   */
  const arc = (toX, toY) => `A${r} ${r} 0 0 1 ${toX} ${toY}`;
  return (
    `path(evenodd, "M0 0H${box.width}V${box.height}H0Z` +
    `M${x + r} ${y}H${right - r}${arc(right, y + r)}V${bottom - r}${arc(right - r, bottom)}` +
    `H${x + r}${arc(x, bottom - r)}V${y + r}${arc(x + r, y)}Z")`
  );
}

/**
 * Follows an element's box on screen and reports where a native surface
 * should sit beneath it.
 */
export class LayoutTracker {
  #element;
  #style;
  #cutout;
  #cutoutClip;
  #cutoutPath;
  #place;
  #resizes;
  #restyles;
  #stopMoves;
  /** @type {Element[]} */
  #ancestors = [];
  /** @type {Element[]} */
  #clippers = [];
  #stale = true;
  #frame = 0;

  /**
   * @param {Element} element
   * @param {HTMLElement | null} cutout
   * @param {(placement: Placement) => void} place
   */
  constructor(element, cutout, place) {
    this.#element = element;
    this.#style = getComputedStyle(element);
    this.#cutout = cutout;
    this.#cutoutClip = cutout?.style.clipPath ?? "";
    this.#cutoutPath = this.#cutoutClip;
    this.#place = place;
    this.#resizes = new ResizeObserver(this.update);
    this.#restyles = new MutationObserver(this.#restyled);
    this.#stopMoves = observeMove(element, this.update);
    for (const name of LAYOUT_EVENTS) {
      addEventListener(name, this.update, { capture: true, passive: true });
    }
    addEventListener("resize", this.#invalidate, { passive: true });
    this.update();
  }

  update = () => {
    if (this.#frame) return;
    this.#frame = requestAnimationFrame(() => {
      this.#frame = 0;
      this.#measure();
    });
  };

  stop() {
    this.#resizes.disconnect();
    this.#restyles.disconnect();
    this.#stopMoves();
    for (const name of LAYOUT_EVENTS) {
      removeEventListener(name, this.update, { capture: true });
    }
    removeEventListener("resize", this.#invalidate);
    cancelAnimationFrame(this.#frame);
    if (this.#cutout) this.#cutout.style.clipPath = this.#cutoutClip;
  }

  #invalidate = () => {
    this.#stale = true;
    this.update();
  };

  /** @param {MutationRecord[]} records */
  #restyled = (records) => {
    if (records.some((record) => record.target !== this.#element)) this.#stale = true;
    this.update();
  };

  #moved() {
    let index = 0;
    for (let parent = this.#element.parentElement; parent; parent = parent.parentElement) {
      if (this.#ancestors[index++] !== parent) return true;
    }
    return index !== this.#ancestors.length;
  }

  #watchAncestors() {
    this.#stale = false;
    this.#ancestors = [];
    for (let parent = this.#element.parentElement; parent; parent = parent.parentElement) {
      this.#ancestors.push(parent);
    }
    this.#clippers = this.#ancestors.filter(clipsOverflow);
    this.#restyles.disconnect();
    for (const element of [this.#element, ...this.#ancestors]) {
      this.#restyles.observe(element, { attributes: true, attributeFilter: STYLE_ATTRIBUTES });
    }
    this.#resizes.disconnect();
    for (const element of [this.#element, ...this.#clippers]) {
      this.#resizes.observe(element);
    }
  }

  #clip() {
    /** @type {Box | null} */
    let clip = { x: 0, y: 0, width: innerWidth, height: innerHeight };
    for (const parent of this.#clippers) {
      const bounds = parent.getBoundingClientRect();
      clip = intersect(clip, {
        x: bounds.left + parent.clientLeft,
        y: bounds.top + parent.clientTop,
        width: parent.clientWidth,
        height: parent.clientHeight,
      });
      if (!clip) return { x: 0, y: 0, width: 0, height: 0 };
    }
    return clip;
  }

  #measure() {
    if (this.#stale || this.#moved()) this.#watchAncestors();
    const element = this.#element;
    const bounds = element.getBoundingClientRect();
    const radius = parseFloat(this.#style.borderTopLeftRadius) || 0;
    const clip = this.#clip();
    const visible = element.isConnected && this.#style.visibility !== "hidden";
    this.#place({
      x: bounds.x,
      y: bounds.y,
      width: bounds.width,
      height: bounds.height,
      radius,
      clip,
      visible,
    });
    if (!this.#cutout) return;
    const hole = visible && intersect(bounds, clip);
    const unclipped = hole && hole.width === bounds.width && hole.height === bounds.height;
    const path = hole ? holePath(this.#cutout, hole, unclipped ? radius : 0) : this.#cutoutClip;
    if (path === this.#cutoutPath) return;
    this.#cutoutPath = path;
    this.#cutout.style.clipPath = path;
    this.#restyles.takeRecords();
  }
}
