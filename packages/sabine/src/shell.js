import { requireApi } from "./bridge.js";

/** @type {typeof import("../types/shell.js").shell} */
export const shell = {
  openLink(url) {
    return requireApi().bridge.invoke("sabine.shell.openLink", { url: String(url) });
  },
  showFolder(path) {
    return requireApi().bridge.invoke("sabine.shell.showFolder", { path: String(path) });
  },
};
