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
 * @template [T=unknown]
 * @param {string} name
 * @param {Record<string, unknown>} [params]
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

export const bridge = {
  /** @returns {string[]} */
  commands() {
    return requireBridge().commands.slice();
  },
  invoke,
  listen,
};

export const app = {
  /** @returns {Promise<string[]>} */
  takeOpenUrls() {
    return invoke("app.takeOpenUrls");
  },
};
