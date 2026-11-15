// `window.sabine.system`'s power state and idle watches. A watch belongs to
// the document that began it, as an activity does, and ends with it.

Object.assign(window.sabine.system, {
  power() { return window.sabine.bridge.invoke("sabine.power.state"); },
  async watchIdle(seconds, callback) {
    const { id } = await window.sabine.bridge.invoke("sabine.idle.watch", { seconds: Number(seconds), document: activityDocument });
    const stopListening = window.sabine.bridge.listen("system.idle", ({ watch, state }) => {
      if (watch === id) callback(state);
    });
    let stopped = false;
    return () => {
      if (stopped) return;
      stopped = true;
      stopListening();
      window.sabine.bridge.send("sabine.idle.unwatch", { id });
    };
  },
});
