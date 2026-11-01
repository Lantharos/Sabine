import { invoke } from "../bridge.js";

/** @type {typeof import("../../types/desktop/shell.js").shell} */
export const shell = {
  async openLink(url) {
    await invoke("sabine.shell.openLink", { url: String(url) });
  },
  async showFolder(path) {
    await invoke("sabine.shell.showFolder", { path: String(path) });
  },
};
