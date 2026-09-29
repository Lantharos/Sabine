const LAYOUT_EVENTS = ["scroll", "resize", "fullscreenchange", "transitionend", "animationend"];

function intersect(a, b) {
  const x = Math.max(a.x, b.x);
  const y = Math.max(a.y, b.y);
  const width = Math.min(a.x + a.width, b.x + b.width) - x;
  const height = Math.min(a.y + a.height, b.y + b.height) - y;
  return width > 0 && height > 0 ? { x, y, width, height } : null;
}

function clipOf(element) {
  let clip = { x: 0, y: 0, width: innerWidth, height: innerHeight };
  for (let parent = element.parentElement; parent && clip; parent = parent.parentElement) {
    const style = getComputedStyle(parent);
    if (style.overflowX === "visible" && style.overflowY === "visible") continue;
    const bounds = parent.getBoundingClientRect();
    clip = intersect(clip, {
      x: bounds.left + parent.clientLeft,
      y: bounds.top + parent.clientTop,
      width: parent.clientWidth,
      height: parent.clientHeight,
    });
  }
  return clip ?? { x: 0, y: 0, width: 0, height: 0 };
}

function holePath(container, hole, radius) {
  const box = container.getBoundingClientRect();
  const x = hole.x - box.x;
  const y = hole.y - box.y;
  const right = x + hole.width;
  const bottom = y + hole.height;
  const r = Math.min(radius, hole.width / 2, hole.height / 2);
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
  #cutout;
  #cutoutClip;
  #place;
  #observer;
  #frame = 0;

  /**
   * @param {Element} element
   * @param {Element | null} cutout
   * @param {(placement: Record<string, unknown>) => void} place
   */
  constructor(element, cutout, place) {
    this.#element = element;
    this.#cutout = cutout;
    this.#cutoutClip = cutout?.style.clipPath ?? "";
    this.#place = place;
    this.#observer = new ResizeObserver(this.update);
    this.#observer.observe(element);
    for (const name of LAYOUT_EVENTS) {
      addEventListener(name, this.update, { capture: true, passive: true });
    }
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
    this.#observer.disconnect();
    for (const name of LAYOUT_EVENTS) {
      removeEventListener(name, this.update, { capture: true });
    }
    cancelAnimationFrame(this.#frame);
    if (this.#cutout) this.#cutout.style.clipPath = this.#cutoutClip;
  }

  #measure() {
    const element = this.#element;
    const bounds = element.getBoundingClientRect();
    const style = getComputedStyle(element);
    const radius = parseFloat(style.borderTopLeftRadius) || 0;
    const clip = clipOf(element);
    const visible = element.isConnected && style.visibility !== "hidden";
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
    this.#cutout.style.clipPath = hole
      ? holePath(this.#cutout, hole, unclipped ? radius : 0)
      : this.#cutoutClip;
  }
}
