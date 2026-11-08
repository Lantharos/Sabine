export const SITE = "https://sabine.rs";
export const BADGE_ALT = "Built with Sabine";

export type BadgeSize = "full" | "compact";
export type BadgeColour = "dark" | "light";
export type BadgeTheme = BadgeColour | "auto";

const heights: Record<BadgeSize, number> = { full: 40, compact: 20 };

export const badgeFile = (size: BadgeSize, colour: BadgeColour, type: "svg" | "png" = "svg") =>
  `/brand/built-with-sabine${size === "compact" ? "-compact" : ""}-${colour}.${type}`;

const badgeUrl = (size: BadgeSize, colour: BadgeColour) => `${SITE}${badgeFile(size, colour)}`;

const image = (size: BadgeSize, colour: BadgeColour) =>
  `<img src="${badgeUrl(size, colour)}" alt="${BADGE_ALT}" height="${heights[size]}">`;

const picture = (size: BadgeSize) =>
  [
    "<picture>",
    `    <source media="(prefers-color-scheme: dark)" srcset="${badgeUrl(size, "light")}">`,
    `    ${image(size, "dark")}`,
    "  </picture>",
  ].join("\n");

export const badgeHtml = (size: BadgeSize, theme: BadgeTheme) =>
  `<a href="${SITE}">\n  ${theme === "auto" ? picture(size) : image(size, theme)}\n</a>`;

export const badgeMarkdown = (size: BadgeSize, colour: BadgeColour) =>
  `[![${BADGE_ALT}](${badgeUrl(size, colour)})](${SITE})`;
