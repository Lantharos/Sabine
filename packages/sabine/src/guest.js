/** @typedef {import("../types/guest.js").GuestBounds} GuestBounds */
/** @typedef {import("../types/guest.js").GuestCreateOptions} GuestCreateOptions */
/** @typedef {import("../types/guest.js").GuestNavigateTarget} GuestNavigateTarget */

import { forward, requireApi } from "./bridge.js";

const guests = () => requireApi().guest;

/** Handle for a guest surface created through the Sabine bridge. */
export class Guest {
  /** @param {string} id */
  constructor(id) {
    this.id = String(id);
  }

  /**
   * @param {GuestCreateOptions} options
   * @returns {Promise<Guest>}
   */
  static async create(options) {
    const { id } = await guests().create(options);
    return new Guest(id);
  }

  get() {
    return guests().get(this.id);
  }

  /** @param {GuestNavigateTarget} target */
  navigate(target) {
    return guests().navigate(this.id, target);
  }

  /** @param {GuestBounds} bounds */
  setBounds(bounds) {
    return guests().setBounds(this.id, bounds);
  }

  /** @param {boolean} visible */
  setVisible(visible) {
    return guests().setVisible(this.id, visible);
  }

  focus() {
    return guests().focus(this.id);
  }

  /** @param {{ ignoreCache?: boolean }} [options] */
  reload(options = {}) {
    return guests().reload(this.id, options);
  }

  goBack() {
    return guests().goBack(this.id);
  }

  goForward() {
    return guests().goForward(this.id);
  }

  /** @param {number} factor */
  setZoom(factor) {
    return guests().setZoom(this.id, factor);
  }

  /** @param {string} code */
  executeJavaScript(code) {
    return guests().executeJavaScript(this.id, code);
  }

  capturePreview() {
    return guests().capturePreview(this.id);
  }

  destroy() {
    return guests().destroy(this.id);
  }
}

/** @type {import("../types/guest.js").GuestHelpers} */
export const guest = /* @__PURE__ */ forward((api) => api.guest, {
  /** @param {GuestCreateOptions} options */
  create: (options) => Guest.create(options),
});
