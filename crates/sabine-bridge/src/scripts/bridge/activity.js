// `window.sabine.activity`. Activities belong to the document that began
// them: they end when it goes away, and when a document replaces it after a
// reload or a renderer crash.

const activityDocument = crypto.randomUUID();
const tellDocument = (name) => window.sabine.bridge.send(name, { document: activityDocument });
tellDocument("sabine.activity.documentOpened");
window.addEventListener("pagehide", () => tellDocument("sabine.activity.documentClosed"));

window.sabine.activity = {
  begin(options = {}) {
    return window.sabine.bridge.invoke("sabine.activity.begin", { ...options, document: activityDocument }).then((record) => {
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
