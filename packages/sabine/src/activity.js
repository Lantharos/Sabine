import { requireApi } from "./bridge.js";

/** @type {import("../types/activity.js").SabineActivityApi} */
export const activity = {
  begin(options = {}) {
    return requireApi().activity.begin(options);
  },
  list() {
    return requireApi().activity.list();
  },
};
