import { forward } from "../bridge.js";

/** @type {import("../../types/desktop/menu.js").SabineMenuApi} */
export const menu = /* @__PURE__ */ forward((api) => {
  if (!api.menu) {
    throw new Error("Menus are only available to the app's own pages in a Sabine window.");
  }
  return api.menu;
});

/** @type {import("../../types/desktop/dialog.js").SabineDialogApi} */
export const dialog = /* @__PURE__ */ forward((api) => {
  if (!api.dialog) {
    throw new Error("Dialogs are only available to the app's own pages in a Sabine window.");
  }
  return api.dialog;
});
