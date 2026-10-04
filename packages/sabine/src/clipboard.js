/** @typedef {import("../types/clipboard.js").SabineClipboardApi} SabineClipboardApi */

/** @type {SabineClipboardApi} */
const browserClipboard = {
  async read({ selection = "clipboard", types } = {}) {
    if (selection !== "clipboard") {
      throw new Error("The primary selection is only available on Linux.");
    }
    /** @type {import("../types/clipboard.js").ClipboardData} */
    const data = {};
    for (const item of await navigator.clipboard.read()) {
      for (const type of item.types) {
        if (types && !types.includes(type)) continue;
        const blob = await item.getType(type);
        data[type] = type.startsWith("text/")
          ? await blob.text()
          : new Uint8Array(await blob.arrayBuffer());
      }
    }
    return data;
  },
  async write(data, { selection = "clipboard" } = {}) {
    if (selection !== "clipboard") {
      throw new Error("The primary selection is only available on Linux.");
    }
    const blobs = Object.entries(data).map(([type, value]) => [
      type,
      new Blob([/** @type {BlobPart} */ (value)], { type }),
    ]);
    await navigator.clipboard.write([new ClipboardItem(Object.fromEntries(blobs))]);
  },
};

/** @returns {SabineClipboardApi} */
function currentClipboard() {
  return globalThis.window?.sabine?.clipboard ?? browserClipboard;
}

/** @type {SabineClipboardApi} */
export const clipboard = {
  read(options) {
    return currentClipboard().read(options);
  },
  write(data, options) {
    return currentClipboard().write(data, options);
  },
};
