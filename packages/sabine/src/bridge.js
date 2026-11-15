/** @typedef {import("../types/bridge.js").SabineApi} SabineApi */
/** @typedef {import("../types/bridge.js").SabineBridge} SabineBridge */
/** @typedef {import("../types/bridge.js").InvokeOptions} InvokeOptions */

/** @returns {SabineApi} */
export function requireApi() {
  const api = globalThis.window?.sabine;
  if (!api) {
    throw new Error(
      "window.sabine is missing. This package only works inside a Sabine window.",
    );
  }
  return api;
}

/** @returns {SabineBridge} */
function requireBridge() {
  const bridge = requireApi().bridge;
  if (!bridge?.__native) {
    throw new Error(
      "Sabine bridge is not available. This package only works inside a Sabine window.",
    );
  }
  return bridge;
}

/** @returns {boolean} */
export function isAvailable() {
  return Boolean(globalThis.window?.sabine?.bridge?.__native);
}

/**
 * URL for a local file in an app that enables local file access.
 * @param {string} path Absolute path to the file.
 * @returns {string}
 */
export function fileUrl(path) {
  const normalized = path.replaceAll("\\", "/");
  const absolute = normalized.startsWith("/") ? normalized : `/${normalized}`;
  return `sabine://file${absolute.split("/").map(encodeURIComponent).join("/")}`;
}

/** @returns {SabineApi} */
export function sabine() {
  return requireApi();
}

/**
 * The part of `window.sabine` that `select` picks, with `own` members on top,
 * looked up whenever a member is used so the package imports anywhere.
 * @template {object} T
 * @template {object} [O={}]
 * @param {(api: SabineApi) => T} select
 * @param {O} [own]
 * @returns {Omit<T, keyof O> & O}
 */
export function forward(select, own = /** @type {O} */ ({})) {
  return /** @type {Omit<T, keyof O> & O} */ (
    new Proxy(own, {
      get: (own, key) => (key in own ? Reflect.get(own, key) : Reflect.get(select(requireApi()), key)),
    })
  );
}

/**
 * @template [T=unknown]
 * @param {string} name
 * @param {object} [params]
 * @param {InvokeOptions} [options]
 * @returns {Promise<T>}
 */
export function invoke(name, params = {}, options = {}) {
  return /** @type {Promise<T>} */ (requireBridge().invoke(name, params, options));
}

/**
 * @template [T=unknown]
 * @param {string} name
 * @param {(payload: T) => void} callback
 * @returns {() => void}
 */
export function listen(name, callback) {
  return requireBridge().listen(name, /** @type {(payload: unknown) => void} */ (callback));
}

/**
 * @param {string} name
 * @param {object} [params]
 */
function send(name, params = {}) {
  requireBridge().send(name, params);
}

export const bridge = {
  /** @returns {string[]} */
  commands() {
    return requireBridge().commands.slice();
  },
  invoke,
  send,
  listen,
};

export const app = {
  /** @returns {Promise<string[]>} */
  takeOpenUrls() {
    return invoke("app.takeOpenUrls");
  },
  /** @returns {Promise<void>} */
  quit() {
    return invoke("app.quit");
  },
  /**
   * @param {boolean} enabled
   * @returns {Promise<void>}
   */
  setAutostart(enabled) {
    return invoke("app.setAutostart", { enabled });
  },
  /**
   * @param {number | string | null} badge
   * @returns {Promise<void>}
   */
  setBadge(badge) {
    return invoke("app.setBadge", { badge });
  },
  /**
   * @param {import("../types/bridge.js").AppProgress} progress
   * @returns {Promise<void>}
   */
  setProgress(progress) {
    if (progress === null) return invoke("app.setProgress", { progress: { state: "hidden" } });
    if (typeof progress === "number") return invoke("app.setProgress", { progress: { state: "normal", value: progress } });
    return invoke("app.setProgress", { progress });
  },
  /**
   * @param {string} path
   * @returns {Promise<void>}
   */
  addRecentDocument(path) {
    return invoke("app.addRecentDocument", { path: String(path) });
  },
  /** @returns {Promise<void>} */
  clearRecentDocuments() {
    return invoke("app.clearRecentDocuments");
  },
};
