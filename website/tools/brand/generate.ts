import { mkdirSync, writeFileSync } from "node:fs";
import { iconSource, png, type Scheme } from "./art.ts";
import { type BadgeSize, badge } from "./badge.ts";
import { mark, wordmark } from "./wordmark.ts";

const root = new URL("../../", import.meta.url).pathname;
const brand = `${root}public/brand/`;
const backdrops: Record<string, Scheme> = { "on-light": "light", "on-dark": "dark" };
const badgeSchemes: Scheme[] = ["light", "dark"];
const badgeSizes: BadgeSize[] = ["full", "compact"];

function write(name: string, contents: string | Uint8Array) {
  writeFileSync(`${brand}${name}`, contents);
  console.log(`public/brand/${name}`);
}

const titled = (source: string) => source.replace(/(<svg[^>]*>)/, "$1<title>Sabine</title>").replace(/>\s+</g, "><");

mkdirSync(brand, { recursive: true });

const icon = titled(iconSource("sabine"));
write("sabine-icon.svg", icon);
for (const size of [128, 256, 512, 1024]) write(`sabine-icon-${size}.png`, png(icon, size));
const small = titled(iconSource("sabine-small"));
write("sabine-icon-small.svg", small);
write("sabine-icon-small-32.png", png(small, 32));

for (const [backdrop, scheme] of Object.entries(backdrops)) {
  write(`sabine-mark-${backdrop}.svg`, mark(scheme));
  write(`sabine-mark-${backdrop}.png`, png(mark(scheme), 512));
  write(`sabine-wordmark-${backdrop}.svg`, wordmark(scheme));
  write(`sabine-wordmark-${backdrop}.png`, png(wordmark(scheme), 1200));
}

for (const size of badgeSizes) {
  for (const scheme of badgeSchemes) {
    const name = `built-with-sabine${size === "compact" ? "-compact" : ""}-${scheme}`;
    const source = badge(size, scheme);
    write(`${name}.svg`, source);
    write(`${name}.png`, png(source, Number(source.match(/width="(\d+)"/)![1]) * 3));
  }
}
