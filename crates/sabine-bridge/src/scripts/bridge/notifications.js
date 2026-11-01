// Chromium has no notification service of its own here, so `Notification`
// shows the desktop's notifications through the window. Not every desktop
// reports a notification people dismissed, so the page keeps only the
// latest ones it may still hear about.

const MAX_NOTIFICATIONS = 64;

if (commands.has("sabine.notification.show")) {
  const shown = new Map();
  const relay = (type) => ({ id, message }) => shown.get(id)?.relay(type, message);
  window.sabine.bridge.listen("notification.click", relay("click"));
  window.sabine.bridge.listen("notification.close", relay("close"));
  window.sabine.bridge.listen("notification.error", relay("error"));
  const remember = (notification) => {
    shown.get(notification.id)?.relay("close");
    shown.set(notification.id, notification);
    if (shown.size > MAX_NOTIFICATIONS) shown.delete(shown.keys().next().value);
  };
  class SabineNotification extends EventTarget {
    static get permission() { return "granted"; }
    static requestPermission(callback) {
      callback?.("granted");
      return Promise.resolve("granted");
    }
    static get maxActions() { return 0; }
    constructor(title, options = {}) {
      super();
      this.title = String(title);
      this.body = String(options.body ?? "");
      this.tag = String(options.tag ?? "");
      this.data = options.data ?? null;
      this.silent = Boolean(options.silent);
      this.onclick = this.onclose = this.onerror = this.onshow = null;
      this.id = this.tag ? "tag:" + this.tag : crypto.randomUUID();
      remember(this);
      const { id, body, silent } = this;
      const urgency = options.urgency === undefined ? undefined : String(options.urgency);
      const category = options.category === undefined ? undefined : String(options.category);
      window.sabine.bridge.invoke("sabine.notification.show", { id, title: this.title, body, silent, urgency, category }).then(
        () => this.relay("show"),
        (error) => this.relay("error", error.message),
      );
    }
    close() {
      window.sabine.bridge.invoke("sabine.notification.close", { id: this.id }).catch(() => {});
    }
    relay(type, message) {
      if (type !== "show" && shown.get(this.id) === this) shown.delete(this.id);
      const event = type === "error" ? new ErrorEvent(type, { message }) : new Event(type, { cancelable: true });
      this["on" + type]?.call(this, event);
      this.dispatchEvent(event);
    }
  }
  const queryPermission = navigator.permissions.query.bind(navigator.permissions);
  navigator.permissions.query = async (descriptor) => {
    const status = await queryPermission(descriptor);
    if (descriptor?.name === "notifications") {
      Object.defineProperty(status, "state", { value: "granted" });
    }
    return status;
  };
  // Chromium installs its own `Notification` once this script has run.
  setTimeout(() => {
    Object.defineProperty(window, "Notification", { value: SabineNotification, configurable: true, writable: true });
  });
}
