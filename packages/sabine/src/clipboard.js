/** @typedef {import("../types/clipboard.js").SabineClipboardApi} SabineClipboardApi */

/** @returns {SabineClipboardApi} */
function requireClipboard() {
  const clipboard = globalThis.window?.sabine?.clipboard;
  if (!clipboard) {
    throw new Error("The clipboard is only available to the app's own pages in a Sabine window.");
  }
  return clipboard;
}

/** @type {SabineClipboardApi} */
export const clipboard = {
  read(options) {
    return requireClipboard().read(options);
  },
  write(data, options) {
    return requireClipboard().write(data, options);
  },
};
