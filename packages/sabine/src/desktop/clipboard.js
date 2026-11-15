import { forward } from "../bridge.js";

/** @type {import("../../types/desktop/clipboard.js").SabineClipboardApi} */
export const clipboard = /* @__PURE__ */ forward((api) => {
  if (!api.clipboard) {
    throw new Error("The clipboard is only available to the app's own pages in a Sabine window.");
  }
  return api.clipboard;
});
