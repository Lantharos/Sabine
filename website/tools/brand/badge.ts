import { ink, round, SWALLOW_BOX, type Scheme, swallow } from "./art.ts";
import { capHeight, letter, weights } from "./lettering.ts";

const BADGE_TITLE = "Built with Sabine";

const sizes = {
  full: { height: 40, mark: 18, text: 15, start: 14, gap: 8, end: 17 },
  compact: { height: 20, mark: 10, text: 11, start: 7, gap: 5, end: 8 },
};

export type BadgeSize = keyof typeof sizes;

export function badge(size: BadgeSize, scheme: Scheme) {
  const { height, mark, text, start, gap, end } = sizes[size];
  const colors = ink[scheme];
  const scale = mark / SWALLOW_BOX.height;
  const markX = start - SWALLOW_BOX.x * scale;
  const markY = (height - mark) / 2 - SWALLOW_BOX.y * scale;
  const baseline = height / 2 + capHeight(weights.semibold, text) / 2;
  const x = start + SWALLOW_BOX.width * scale + gap;
  const lead = letter({ text: "Built with ", font: weights.medium, size: text, x, baseline });
  const name = letter({ text: "Sabine", font: weights.semibold, size: text, x: lead.end, baseline, tracking: -0.01 });
  const width = Math.ceil(name.right + end);
  const placed = `matrix(${round(scale)} 0 0 ${round(scale)} ${round(markX)} ${round(markY)})`;
  return (
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} ${height}" width="${width}" height="${height}">` +
    `<title>${BADGE_TITLE}</title>` +
    `<rect width="${width}" height="${height}" rx="${height / 2}" fill="${colors.fill}"/>` +
    swallow(colors.strong, placed) +
    `<path d="${lead.d}" fill="${colors.soft}"/>` +
    `<path d="${name.d}" fill="${colors.strong}"/>` +
    `</svg>\n`
  );
}
