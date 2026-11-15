import { forward } from "./bridge.js";

/** @type {import("../types/activity.js").SabineActivityApi} */
export const activity = /* @__PURE__ */ forward((api) => api.activity);
