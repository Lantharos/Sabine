// The `window.sabine` page API. The native host defines
// `window.__sabineBridgeCommands` and `window.__sabineNativePostMessage`
// before running it, and answers through `__sabineBridgeResolve` and
// `__sabineBridgeEmit`. `INSTALL_SCRIPT` in lib.rs runs the files of this
// folder in one function, this one first, so the others use what it declares.

const commands = new Set(window.__sabineBridgeCommands || []);
const pending = new Map();
const listeners = new Map();
let nextId = 1;

const received = (payload) => payload instanceof ArrayBuffer ? new Uint8Array(payload) : payload;

window.__sabineBridgeResolve = function (id, ok, payload) {
  const entry = pending.get(String(id));
  if (!entry) return;
  entry.cleanup();
  if (ok) {
    entry.resolve(received(payload));
  } else {
    entry.reject(new Error((payload && payload.message) || "Sabine bridge command failed"));
  }
};

window.__sabineBridgeEmit = function (name, value) {
  const set = listeners.get(String(name));
  if (!set) return;
  const payload = received(value);
  for (const cb of Array.from(set)) {
    queueMicrotask(() => cb(payload));
  }
};

const postNative = function (...message) {
  if (typeof window.__sabineNativePostMessage !== "function") {
    throw new Error("Sabine native transport is unavailable");
  }
  window.__sabineNativePostMessage(...message);
};

const requireCommand = function (name) {
  if (!commands.has(name)) {
    throw new Error("Sabine bridge command is not available to this page: " + name);
  }
};

window.sabine = window.sabine || {};
window.sabine.bridge = {
  __native: true,
  commands: Array.from(commands),
  listen(name, callback) {
    const key = String(name);
    let set = listeners.get(key);
    if (!set) { set = new Set(); listeners.set(key, set); }
    set.add(callback);
    return () => {
      set.delete(callback);
      if (!set.size) listeners.delete(key);
    };
  },
  async invoke(name, params = {}, options = {}) {
    requireCommand(name);
    const { signal, timeoutMs = 60000, body } = options;
    signal?.throwIfAborted();
    if (!Number.isFinite(timeoutMs) || timeoutMs <= 0 || timeoutMs > 2147483647) {
      throw new RangeError("Sabine bridge timeoutMs must be between 1 and 2147483647");
    }
    if (pending.size >= 128) {
      throw new Error("Sabine bridge request capacity is exhausted");
    }
    const id = String(nextId++);
    const payload = JSON.stringify(params);
    const bytes = body === undefined ? undefined
      : body instanceof Blob ? await body.arrayBuffer()
      : ArrayBuffer.isView(body) ? body.buffer.slice(body.byteOffset, body.byteOffset + body.byteLength)
      : body;
    return new Promise((resolve, reject) => {
      const cleanup = () => {
        pending.delete(id);
        clearTimeout(timer);
        signal?.removeEventListener("abort", abort);
      };
      const cancel = (reason) => {
        cleanup();
        try { postNative("cancel", id); } catch {}
        reject(reason);
      };
      const abort = () => cancel(signal.reason);
      const timer = setTimeout(() => {
        cancel(new DOMException("Sabine bridge command timed out: " + name, "TimeoutError"));
      }, timeoutMs);
      pending.set(id, { resolve, reject, cleanup, cancel });
      signal?.addEventListener("abort", abort, { once: true });
      try {
        postNative("bridge", id, name, payload, ...(bytes === undefined ? [] : [bytes]));
      } catch (error) {
        cleanup();
        reject(error);
      }
    });
  },
  send(name, params = {}) {
    requireCommand(name);
    postNative("send", name, JSON.stringify(params));
  },
};

window.sabine.bridge.listen("sabine.bridge.dropped", ({ command }) => {
  console.warn("Sabine dropped updates sent to " + command + " while the app's handlers were busy");
});

window.addEventListener("pagehide", () => {
  for (const entry of pending.values()) {
    entry.cancel(new DOMException("Sabine page was hidden", "AbortError"));
  }
});

const bounds = function (value = {}) {
  return {
    x: Math.round(Number(value.x) || 0),
    y: Math.round(Number(value.y) || 0),
    width: Math.max(1, Math.round(Number(value.width) || 1)),
    height: Math.max(1, Math.round(Number(value.height) || 1)),
  };
};
