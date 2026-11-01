import { requireApi } from "../bridge.js";

/** @type {typeof import("../../types/desktop/print.js").print} */
export const print = {
  page() {
    return requireApi().print.page();
  },
  element(element) {
    return requireApi().print.element(element);
  },
  files(paths, options) {
    return requireApi().print.files(paths, options);
  },
};
