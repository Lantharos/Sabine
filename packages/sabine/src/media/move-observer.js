import { sameBox } from "./box.js";

/**
 * Calls `onMove` whenever `element` moves on screen, for whatever reason,
 * such as content above it growing. An `IntersectionObserver` whose root is
 * shrunk to the element's own box notices it leaving that box, and is then
 * shrunk to the new one.
 * @param {Element} element
 * @param {() => void} onMove
 * @returns {() => void} Stops observing.
 */
export function observeMove(element, onMove) {
  /** @type {IntersectionObserver | null} */
  let observer = null;
  let retry = 0;

  const stop = () => {
    clearTimeout(retry);
    observer?.disconnect();
    observer = null;
  };

  /**
   * @param {boolean} moved
   * @param {number} threshold
   */
  const watch = (moved, threshold) => {
    stop();
    if (moved) onMove();
    const box = element.getBoundingClientRect();
    const { left, top, width, height } = box;
    if (!width || !height) return;
    const root = document.documentElement;
    const inset = [
      top,
      root.clientWidth - (left + width),
      root.clientHeight - (top + height),
      left,
    ].map((side) => `${-Math.floor(side)}px`);
    let first = true;
    observer = new IntersectionObserver(
      ([entry]) => {
        const ratio = entry.intersectionRatio;
        if (ratio !== threshold) {
          if (!first) return watch(true, 1);
          if (ratio) watch(true, ratio);
          else retry = setTimeout(() => watch(true, 1e-7), 1000);
          return;
        }
        if (ratio === 1 && !sameBox(box, element.getBoundingClientRect())) return watch(true, 1);
        first = false;
      },
      { root: document, rootMargin: inset.join(" "), threshold: Math.min(1, threshold) || 1 },
    );
    observer.observe(element);
  };

  watch(false, 1);
  return stop;
}
