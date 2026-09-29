// The `window.sabine` page API. The native host embeds this file at build time,
// defines `window.__sabineBridgeCommands` and `window.__sabineNativePostMessage`
// before running it, and answers through `__sabineBridgeResolve` and
// `__sabineBridgeEmit`. Edit only this file; there is no other copy.

(function () {
  if (window.sabine && window.sabine.bridge && window.sabine.bridge.__native) return;
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
    const payload = received(value);
    const set = listeners.get(String(name));
    if (set) {
      for (const cb of Array.from(set)) {
        queueMicrotask(() => cb(payload));
      }
    }
    window.dispatchEvent(new CustomEvent("sabine:" + String(name), { detail: payload }));
  };

  const postNative = function (...message) {
    if (typeof window.__sabineNativePostMessage !== "function") {
      throw new Error("Sabine native transport is unavailable");
    }
    window.__sabineNativePostMessage(...message);
  };

  const windowCommand = function (action, value) {
    postNative("window", action, value == null ? "" : String(value));
  };

  window.sabine = window.sabine || {};
  window.sabine.window = Object.assign(window.sabine.window || {}, {
    show() { windowCommand("show"); },
    hide() { windowCommand("hide"); },
    focus(activationToken) { windowCommand("focus", activationToken); },
    close() { windowCommand("close"); },
    minimize() { windowCommand("minimize"); },
    maximize() { windowCommand("maximize"); },
    toggleMaximize() { windowCommand("toggle-maximize"); },
    setFullscreen(enabled) { windowCommand(enabled ? "fullscreen" : "exit-fullscreen"); },
    restore() { windowCommand("restore"); },
    startDrag() { windowCommand("start-drag"); },
    async inhibitShortcuts(enabled) {
      await window.sabine.bridge.invoke("sabine.window.inhibitShortcuts", { enabled: Boolean(enabled) });
    },
  });

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
      if (!commands.has(name)) {
        throw new Error("Sabine bridge command not registered: " + name);
      }
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
  };

  window.addEventListener("pagehide", () => {
    for (const entry of pending.values()) {
      entry.cancel(new DOMException("Sabine page was hidden", "AbortError"));
    }
  });

  window.sabine.activity = {
    begin(options = {}) {
      return window.sabine.bridge.invoke("sabine.activity.begin", options).then((record) => {
        let ended = false;
        return Object.assign({}, record, {
          end() {
            if (ended) return Promise.resolve({ id: record.id, ended: false });
            ended = true;
            return window.sabine.bridge.invoke("sabine.activity.end", { id: record.id });
          },
        });
      });
    },
    list() { return window.sabine.bridge.invoke("sabine.activity.list"); },
  };

  if (commands.has("sabine.popup.open") && commands.has("sabine.popup.close")) {
    window.sabine.popup = Object.assign(window.sabine.popup || {}, {
      open(options = {}) {
        return window.sabine.bridge.invoke("sabine.popup.open", {
          x: Math.round(Number(options.x) || 0),
          y: Math.round(Number(options.y) || 0),
          width: Math.max(1, Math.round(Number(options.width) || 1)),
          height: Math.max(1, Math.round(Number(options.height) || 1)),
          html: String(options.html || ""),
          url: String(options.url || ""),
        });
      },
      close() {
        return window.sabine.bridge.invoke("sabine.popup.close");
      },
    });
  }

  if (commands.has("sabine.guest.create")) {
    const guestBounds = function (options) {
      const bounds = options.bounds || options;
      return {
        x: Math.round(Number(bounds.x) || 0),
        y: Math.round(Number(bounds.y) || 0),
        width: Math.max(1, Math.round(Number(bounds.width) || 1)),
        height: Math.max(1, Math.round(Number(bounds.height) || 1)),
      };
    };

    window.sabine.guest = Object.assign(window.sabine.guest || {}, {
      create(options = {}) {
        const bounds = guestBounds(options);
        return window.sabine.bridge.invoke("sabine.guest.create", {
          id: options.id ? String(options.id) : undefined,
          url: options.url ? String(options.url) : undefined,
          html: options.html ? String(options.html) : undefined,
          x: bounds.x,
          y: bounds.y,
          width: bounds.width,
          height: bounds.height,
          bounds,
          partition: options.partition ? String(options.partition) : undefined,
          allowBridge: Boolean(options.allowBridge),
          interceptedShortcuts: (options.interceptedShortcuts || []).map(String),
          interceptHorizontalWheel: Boolean(options.interceptHorizontalWheel),
          visible: options.visible === undefined ? true : Boolean(options.visible),
          popupPolicy: String(options.popupPolicy || "deny"),
          allowDownloads:
            options.allowDownloads === undefined ? true : Boolean(options.allowDownloads),
          backgroundColor: options.backgroundColor
            ? String(options.backgroundColor)
            : undefined,
        });
      },
      destroy(id) {
        return window.sabine.bridge.invoke("sabine.guest.destroy", { id: String(id) });
      },
      navigate(id, url) {
        return window.sabine.bridge.invoke("sabine.guest.navigate", {
          id: String(id),
          url: String(url),
        });
      },
      setBounds(id, bounds) {
        const next = guestBounds(bounds || {});
        return window.sabine.bridge.invoke("sabine.guest.setBounds", {
          id: String(id),
          x: next.x,
          y: next.y,
          width: next.width,
          height: next.height,
          bounds: next,
        });
      },
      setVisible(id, visible) {
        return window.sabine.bridge.invoke("sabine.guest.setVisible", {
          id: String(id),
          visible: Boolean(visible),
        });
      },
      setCovered(covered) {
        return window.sabine.bridge.invoke("sabine.guest.setCovered", {
          covered: Boolean(covered),
        });
      },
      capturePreview(id) {
        return window.sabine.bridge.invoke("sabine.guest.capturePreview", {
          id: String(id),
        });
      },
      focus(id) {
        return window.sabine.bridge.invoke("sabine.guest.focus", { id: String(id) });
      },
      reload(id, options = {}) {
        return window.sabine.bridge.invoke("sabine.guest.reload", {
          id: String(id),
          ignoreCache: Boolean(options.ignoreCache),
        });
      },
      goBack(id) {
        return window.sabine.bridge.invoke("sabine.guest.goBack", { id: String(id) });
      },
      goForward(id) {
        return window.sabine.bridge.invoke("sabine.guest.goForward", { id: String(id) });
      },
      setZoom(id, factor) {
        return window.sabine.bridge.invoke("sabine.guest.setZoom", {
          id: String(id),
          factor: Number(factor) || 1,
        });
      },
      executeJavaScript(id, code) {
        return window.sabine.bridge.invoke("sabine.guest.executeJavaScript", {
          id: String(id),
          code: String(code),
        });
      },
      downloadAction(downloadId, action, options = {}) {
        return window.sabine.bridge.invoke("sabine.guest.downloadAction", {
          downloadId: String(downloadId),
          action: String(action),
          savePath: options.savePath ? String(options.savePath) : undefined,
          showDialog: Boolean(options.showDialog),
        });
      },
      list() {
        return window.sabine.bridge.invoke("sabine.guest.list");
      },
      get(id) {
        return window.sabine.bridge.invoke("sabine.guest.get", { id: String(id) });
      },
    });
  }
})();
