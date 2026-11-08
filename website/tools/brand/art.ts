import { readFileSync } from "node:fs";
import { Resvg } from "@resvg/resvg-js";

export const palette = {
  paper: "#f9f4ea",
  cream: "#f3ead8",
  sand: "#e7d9bf",
  slate: "#47557a",
  navy: "#1d2a4a",
  night: "#121a2c",
};

export type Scheme = "light" | "dark";

export const ink: Record<Scheme, { strong: string; soft: string; fill: string }> = {
  light: { strong: palette.navy, soft: palette.slate, fill: palette.cream },
  dark: { strong: palette.paper, soft: palette.sand, fill: palette.navy },
};

const icons = new URL("../../../assets/icon/", import.meta.url).pathname;

export const iconSource = (name: string) => readFileSync(`${icons}${name}.svg`, "utf8");

export const SWALLOW = iconSource("sabine").match(/<path d="([^"]+)" fill="#1d2a4a"\/>/)![1]!;

export const round = (value: number) => Math.round(value * 100) / 100;

export const svg = (viewBox: string, body: string, size = "") =>
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${viewBox}"${size}><title>Sabine</title>${body}</svg>\n`;

const probe = new Resvg(svg("0 0 64 64", `<path d="${SWALLOW}"/>`)).getBBox()!;
export const SWALLOW_BOX = { x: probe.x, y: probe.y, width: probe.width, height: probe.height };

export const swallow = (fill: string, transform = "") =>
  `<path d="${SWALLOW}" fill="${fill}"${transform && ` transform="${transform}"`}/>`;

export const png = (source: string, width: number) =>
  new Resvg(source, { fitTo: { mode: "width", value: width } }).render().asPng();
