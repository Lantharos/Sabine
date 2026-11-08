import { type Font, openSync } from "fontkit";

const fonts = new URL("../../node_modules/@fontsource/hanken-grotesk/files/", import.meta.url).pathname;
const open = (weight: number) => openSync(`${fonts}hanken-grotesk-latin-${weight}-normal.woff2`) as Font;

export const weights = { medium: open(500), semibold: open(600) };

const rounded = (d: string) => d.replace(/-?\d*\.\d+/g, (n) => String(Math.round(Number(n) * 100) / 100));

export type Run = { text: string; font: Font; size: number; x: number; baseline: number; tracking?: number };

export function letter(run: Run) {
  const scale = run.size / run.font.unitsPerEm;
  const layout = run.font.layout(run.text);
  let x = run.x;
  let d = "";
  let right = run.x;
  layout.glyphs.forEach((glyph, index) => {
    const position = layout.positions[index]!;
    const origin = x + position.xOffset * scale;
    d += glyph.path.scale(scale, -scale).translate(origin, run.baseline - position.yOffset * scale).toSVG();
    if (glyph.path.commands.length) right = Math.max(right, origin + glyph.bbox.maxX * scale);
    x += position.xAdvance * scale + (run.tracking ?? 0) * run.size;
  });
  return { d: rounded(d), end: x, right };
}

export const capHeight = (font: Font, size: number) => (font.capHeight / font.unitsPerEm) * size;
