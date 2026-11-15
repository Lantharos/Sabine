// Chromium has no notification service of its own here, so `Notification`
// shows the desktop's notifications through the window. Not every desktop
// reports a notification people dismissed, so the page keeps only the
// latest ones it may still hear about. The page reads its pictures itself,
// so they load as any of its images would, and hands them over as `data:`
// URLs; a picture it can't read, or a large one, is left out.

const MAX_NOTIFICATIONS = 64;
const MAX_NOTIFICATION_ACTIONS = 5;
// Both pictures fit in one bridge call next to the rest of the notification.
const MAX_PICTURE_BYTES = 256 * 1024;

if (commands.has("sabine.notification.show")) {
  const shown = new Map();
  const relay = (type) => (payload) => shown.get(payload.id)?.relay(type, payload);
  window.sabine.bridge.listen("notification.click", relay("click"));
  window.sabine.bridge.listen("notification.close", relay("close"));
  window.sabine.bridge.listen("notification.error", relay("error"));
  const remember = (notification) => {
    shown.get(notification.id)?.relay("close");
    shown.set(notification.id, notification);
    if (shown.size > MAX_NOTIFICATIONS) shown.delete(shown.keys().next().value);
  };
  const picture = async (url) => {
    if (!url) return undefined;
    try {
      const response = await fetch(new URL(url, location.href));
      if (!response.ok) return undefined;
      const blob = await response.blob();
      if (blob.size > MAX_PICTURE_BYTES) return undefined;
      return await new Promise((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => resolve(reader.result);
        reader.onerror = () => reject(reader.error);
        reader.readAsDataURL(blob);
      });
    } catch {
      return undefined;
    }
  };
  const actionsOf = (actions = []) => Array.from(actions).slice(0, MAX_NOTIFICATION_ACTIONS).map((action) => ({
    id: String(action.action),
    label: String(action.title),
    reply: action.type === "text" ? String(action.placeholder ?? "") : undefined,
  }));
  class SabineNotification extends EventTarget {
    static get permission() { return "granted"; }
    static requestPermission(callback) {
      callback?.("granted");
      return Promise.resolve("granted");
    }
    static get maxActions() { return MAX_NOTIFICATION_ACTIONS; }
    constructor(title, options = {}) {
      super();
      this.title = String(title);
      this.body = String(options.body ?? "");
      this.tag = String(options.tag ?? "");
      this.icon = String(options.icon ?? "");
      this.image = String(options.image ?? "");
      this.data = options.data ?? null;
      this.silent = Boolean(options.silent);
      this.actions = Object.freeze(Array.from(options.actions ?? [], (action) => Object.freeze({ ...action })));
      this.onclick = this.onclose = this.onerror = this.onshow = null;
      this.id = this.tag ? "tag:" + this.tag : crypto.randomUUID();
      remember(this);
      const { id, body, silent } = this;
      const urgency = options.urgency === undefined ? undefined : String(options.urgency);
      const category = options.category === undefined ? undefined : String(options.category);
      const actions = actionsOf(options.actions);
      Promise.all([picture(this.icon), picture(this.image)])
        .then(([icon, image]) => window.sabine.bridge.invoke("sabine.notification.show", { id, title: this.title, body, silent, urgency, category, icon, image, actions }))
        .then(
          () => this.relay("show"),
          (error) => this.relay("error", { message: error.message }),
        );
    }
    close() {
      window.sabine.bridge.invoke("sabine.notification.close", { id: this.id }).catch(() => {});
    }
    relay(type, { message, action, reply } = {}) {
      if (type !== "show" && shown.get(this.id) === this) shown.delete(this.id);
      const event = type === "error" ? new ErrorEvent(type, { message }) : new Event(type, { cancelable: true });
      if (type === "click") Object.assign(event, { action: action ?? "", reply: reply ?? null });
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
