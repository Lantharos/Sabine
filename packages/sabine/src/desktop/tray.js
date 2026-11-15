import { invoke } from "../bridge.js";

/** @type {typeof import("../../types/desktop/tray.js").tray} */
export const tray = {
  update({ icon, ...changes }) {
    if (icon === undefined) return invoke("sabine.tray.update", changes);
    if (icon === null || typeof icon === "string") {
      return invoke("sabine.tray.update", { ...changes, icon });
    }
    return invoke("sabine.tray.update", changes, { body: icon });
  },
};
