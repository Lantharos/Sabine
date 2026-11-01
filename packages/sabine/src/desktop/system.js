import { requireApi } from "../bridge.js";

/** @type {typeof import("../../types/desktop/system.js").system} */
export const system = {
  appearance() {
    return requireApi().system.appearance();
  },
};
