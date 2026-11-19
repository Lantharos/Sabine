// `window.sabine.downloads` and `window.sabine.spellcheck`: the downloads the
// page started, and the spelling its text fields check.

const downloadAction = (action) => (downloadId) =>
  window.sabine.bridge.invoke("sabine.download." + action, { downloadId: String(downloadId) });

window.sabine.downloads = {
  pause: downloadAction("pause"),
  resume: downloadAction("resume"),
  cancel: downloadAction("cancel"),
};

window.sabine.spellcheck = {
  get() {
    return window.sabine.bridge.invoke("sabine.spellcheck.get");
  },
  set({ enabled, languages } = {}) {
    return window.sabine.bridge.invoke("sabine.spellcheck.set", {
      enabled: enabled === undefined ? undefined : Boolean(enabled),
      languages: languages === undefined ? undefined : Array.from(languages, String),
    });
  },
  addWord(word) {
    return window.sabine.bridge.invoke("sabine.spellcheck.addWord", { word: String(word) });
  },
  replace(word) {
    return window.sabine.bridge.invoke("sabine.spellcheck.replace", { word: String(word) });
  },
};
