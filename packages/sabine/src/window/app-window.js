import { forward } from "../bridge.js";

/** @type {import("../../types/window/app-window.js").SabineWindowApi} */
export const appWindow = /* @__PURE__ */ forward((api) => api.window);

/** @type {import("../../types/window/app-window.js").SabinePopupApi} */
export const popup = /* @__PURE__ */ forward((api) => api.popup);
