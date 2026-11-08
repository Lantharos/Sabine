import { Resvg } from "@resvg/resvg-js";
import { ink, round, SWALLOW_BOX, type Scheme, svg, swallow } from "./art.ts";
import { letter, weights } from "./lettering.ts";

const EM = 100;
const MARK_HEIGHT = 1.08 * EM;
const GAP = 0.5 * EM;
const HEADER_BOX = { x: 6, y: 10, width: 52, height: 44 };
const font = weights.semibold;

function body(scheme: Scheme) {
  const scale = MARK_HEIGHT / HEADER_BOX.height;
  const textX = HEADER_BOX.width * scale + GAP;
  const middle = MARK_HEIGHT / 2;
  const baseline = middle + ((font.ascent + font.descent) / 2 / font.unitsPerEm) * EM;
  const name = letter({ text: "Sabine", font, size: EM, x: textX, baseline, tracking: -0.02 });
  const placed = `matrix(${round(scale)} 0 0 ${round(scale)} ${round(-HEADER_BOX.x * scale)} ${round(-HEADER_BOX.y * scale)})`;
  return swallow(ink[scheme].strong, placed) + `<path d="${name.d}" fill="${ink[scheme].strong}"/>`;
}

export function fitted(content: string) {
  const box = new Resvg(svg("0 0 1000 1000", content)).getBBox()!;
  return [box.x, box.y, box.width, box.height].map(round).join(" ");
}

export function wordmark(scheme: Scheme) {
  const content = body(scheme);
  return svg(fitted(content), content);
}

export function mark(scheme: Scheme) {
  const { x, y, width, height } = SWALLOW_BOX;
  return svg([x, y, width, height].map(round).join(" "), swallow(ink[scheme].strong));
}
