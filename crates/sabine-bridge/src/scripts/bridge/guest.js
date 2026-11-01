// `window.sabine.guest`: other sites shown in the window, beside its page.

window.sabine.guest = {
  create(options = {}) {
    return window.sabine.bridge.invoke("sabine.guest.create", {
      id: options.id ? String(options.id) : undefined,
      url: options.url ? String(options.url) : undefined,
      html: options.html ? String(options.html) : undefined,
      bounds: bounds(options.bounds),
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
  navigate(id, target) {
    const page = typeof target === "string" ? { url: target } : target || {};
    return window.sabine.bridge.invoke("sabine.guest.navigate", {
      id: String(id),
      url: page.url ? String(page.url) : undefined,
      html: page.html ? String(page.html) : undefined,
    });
  },
  setBounds(id, next) {
    return window.sabine.bridge.invoke("sabine.guest.setBounds", {
      id: String(id),
      bounds: bounds(next),
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
  async list() {
    const { guests } = await window.sabine.bridge.invoke("sabine.guest.list");
    return guests;
  },
  get(id) {
    return window.sabine.bridge.invoke("sabine.guest.get", { id: String(id) });
  },
};
