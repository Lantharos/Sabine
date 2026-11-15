import { forward } from "../bridge.js";

/** @type {import("../../types/desktop/system.js").SabineSystemApi} */
export const system = /* @__PURE__ */ forward((api) => api.system);
