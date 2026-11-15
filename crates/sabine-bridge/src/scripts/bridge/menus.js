// `window.sabine.menu` and `window.sabine.dialog`: the app's items in the
// context menu, menus at a point of the page, the app menu bar, and messages
// over the page, all drawn by the window.

const untilAnswered = { timeoutMs: 2147483647 };

if (commands.has("sabine.menu.popup")) {
  window.sabine.menu = {
    setContextMenu(items) {
      return window.sabine.bridge.invoke("sabine.menu.setContextMenu", { items: Array.from(items) });
    },
    popup(items, { x = 0, y = 0 } = {}) {
      return window.sabine.bridge.invoke(
        "sabine.menu.popup",
        { items: Array.from(items), x: Number(x) || 0, y: Number(y) || 0 },
        untilAnswered,
      );
    },
    setAppMenu(items) {
      return window.sabine.bridge.invoke("sabine.menu.setAppMenu", { items: Array.from(items) });
    },
    appMenu() { return window.sabine.bridge.invoke("sabine.menu.appMenu"); },
    activate(id) { return window.sabine.bridge.invoke("sabine.menu.activate", { id: String(id) }); },
    onSelect(callback) { return window.sabine.bridge.listen("menu.select", callback); },
    onAppMenuChanged(callback) {
      return window.sabine.bridge.listen("menu.appMenu", ({ items }) => callback(items));
    },
  };
}

if (commands.has("sabine.dialog.show")) {
  const show = (dialog) => window.sabine.bridge.invoke("sabine.dialog.show", dialog, untilAnswered);
  const text = (value) => (value == null ? "" : String(value));
  window.sabine.dialog = {
    async message({ title, message, buttons = ["OK"], defaultButton, cancelButton } = {}) {
      const labels = Array.from(buttons, String);
      const answer = await show({
        title: text(title),
        message: text(message),
        buttons: labels,
        defaultButton: defaultButton ?? Math.max(labels.length - 1, 0),
        cancelButton: cancelButton ?? 0,
      });
      return answer.button;
    },
    async confirm({ title, message, okLabel = "OK", cancelLabel = "Cancel" } = {}) {
      const answer = await show({
        title: text(title),
        message: text(message),
        buttons: [String(cancelLabel), String(okLabel)],
        defaultButton: 1,
        cancelButton: 0,
      });
      return answer.button === 1;
    },
    async prompt({ title, message, value, placeholder, okLabel = "OK", cancelLabel = "Cancel" } = {}) {
      const answer = await show({
        title: text(title),
        message: text(message),
        buttons: [String(cancelLabel), String(okLabel)],
        defaultButton: 1,
        cancelButton: 0,
        input: { value: text(value), placeholder: text(placeholder) },
      });
      return answer.button === 1 ? answer.value : null;
    },
  };
}
