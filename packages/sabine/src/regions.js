/** @param {Record<string, unknown>} adaptive */
function adaptiveRegion(adaptive) {
  return { adaptive, rects: [] };
}

/** @type {typeof import("../types/window.js").region} */
export const region = {
  empty() {
    return { rects: [] };
  },
  rect(x, y, width, height) {
    return { rects: [{ x, y, width, height }] };
  },
  full() {
    return adaptiveRegion({ kind: "full" });
  },
  roundedRect(radius) {
    return adaptiveRegion({ kind: "rounded_rect", radius });
  },
  roundedLeft(width, radius) {
    return adaptiveRegion({ kind: "rounded_left", width, radius });
  },
  titlebarAndSidebar(sidebarWidth, titlebarHeight, radius) {
    return adaptiveRegion({
      kind: "titlebar_sidebar",
      sidebar_width: sidebarWidth,
      titlebar_height: titlebarHeight,
      radius,
    });
  },
  contentAfterSidebar(sidebarWidth, titlebarHeight = 0) {
    return adaptiveRegion({
      kind: "content_after_sidebar",
      sidebar_width: sidebarWidth,
      titlebar_height: titlebarHeight,
    });
  },
  contentAfterSidebarRoundedRight(sidebarWidth, titlebarHeight, radius) {
    return adaptiveRegion({
      kind: "content_after_sidebar_rounded_right",
      sidebar_width: sidebarWidth,
      titlebar_height: titlebarHeight,
      radius,
    });
  },
};
