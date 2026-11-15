/** @typedef {{ x: number, y: number, width: number, height: number }} Box */

/**
 * @param {Box} a
 * @param {Box} b
 * @returns {Box | null}
 */
export function intersect(a, b) {
  const x = Math.max(a.x, b.x);
  const y = Math.max(a.y, b.y);
  const width = Math.min(a.x + a.width, b.x + b.width) - x;
  const height = Math.min(a.y + a.height, b.y + b.height) - y;
  return width > 0 && height > 0 ? { x, y, width, height } : null;
}

/**
 * @param {Box | null} a
 * @param {Box | null} b
 */
export function sameBox(a, b) {
  return (
    a === b ||
    (a !== null &&
      b !== null &&
      a.x === b.x &&
      a.y === b.y &&
      a.width === b.width &&
      a.height === b.height)
  );
}
