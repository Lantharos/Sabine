import { forward } from "../bridge.js";

/** @type {import("../../types/page/downloads.js").SabineDownloadsApi} */
export const downloads = /* @__PURE__ */ forward((api) => api.downloads);
