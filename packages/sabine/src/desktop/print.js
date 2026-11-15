import { forward } from "../bridge.js";

/** @type {import("../../types/desktop/print.js").SabinePrintApi} */
export const print = /* @__PURE__ */ forward((api) => api.print);
