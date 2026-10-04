import { invoke } from "./bridge.js";

/** @type {typeof import("../types/tray.js").tray} */
export const tray = {
  update(changes) {
    return invoke("sabine.tray.update", changes);
  },
};
