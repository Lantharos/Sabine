import { forward } from "../bridge.js";

/** @type {import("../../types/page/spellcheck.js").SabineSpellcheckApi} */
export const spellcheck = /* @__PURE__ */ forward((api) => api.spellcheck);
