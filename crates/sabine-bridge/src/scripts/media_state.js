(() => {
  if (window.__sabineMediaStateInstalled) return;
  window.__sabineMediaStateInstalled = true;
  const report = window.__sabineMediaState;
  const elements = new Set();
  const contexts = new Set();
  let reported = false;
  let queued = false;

  const playing = () => {
    let any = false;
    for (const element of elements) {
      if (element.paused || element.ended) elements.delete(element);
      else any = true;
    }
    for (const context of contexts) {
      if (context.state === "running") any = true;
    }
    return any;
  };
  const update = () => {
    queued = false;
    const now = playing();
    if (now !== reported) {
      reported = now;
      report(now ? "1" : "0");
    }
  };
  const schedule = () => {
    if (queued) return;
    queued = true;
    queueMicrotask(update);
  };
  const watch = (element) => {
    if (elements.has(element)) return;
    elements.add(element);
    for (const type of ["playing", "pause", "ended", "emptied"]) {
      element.addEventListener(type, schedule);
    }
  };

  addEventListener("play", (event) => {
    if (event.target instanceof HTMLMediaElement) {
      watch(event.target);
      schedule();
    }
  }, true);
  const play = HTMLMediaElement.prototype.play;
  HTMLMediaElement.prototype.play = function (...args) {
    watch(this);
    const result = play.apply(this, args);
    schedule();
    return result;
  };
  for (const name of ["AudioContext", "webkitAudioContext"]) {
    const Base = window[name];
    if (!Base) continue;
    window[name] = class AudioContext extends Base {
      constructor(...args) {
        super(...args);
        contexts.add(this);
        this.addEventListener("statechange", () => {
          if (this.state === "closed") contexts.delete(this);
          schedule();
        });
        schedule();
      }
    };
  }
  addEventListener("pagehide", () => {
    if (reported) {
      reported = false;
      report("0");
    }
  });
})();
