// Browsers keep the paths of dropped files to themselves. The app's own page
// gets them as `file.path` and as `text/uri-list`, and the file URIs it puts
// in a drag's `text/uri-list` reach other apps as files.

const URI_LIST = "text/uri-list";
const transferTypes = Object.getOwnPropertyDescriptor(DataTransfer.prototype, "types").get;
const getTransferData = DataTransfer.prototype.getData;
const setTransferData = DataTransfer.prototype.setData;
const draggedTransfer = (transfer, type) =>
  window.event instanceof DragEvent && window.event.dataTransfer === transfer && (!type || window.event.type === type);
const fileUri = (path) => {
  const parts = path.replace(/\\/g, "/").split("/").map((part) => /^[A-Za-z]:$/.test(part) ? part : encodeURIComponent(part));
  const joined = parts.join("/");
  return (joined.startsWith("//") ? "file:" : joined.startsWith("/") ? "file://" : "file:///") + joined;
};
let droppedPaths = [];
window.__sabineDropPaths = (paths) => { droppedPaths = paths; };
const carriesDroppedFiles = (transfer) =>
  droppedPaths.length > 0 && draggedTransfer(transfer) && transferTypes.call(transfer).includes("Files");
Object.defineProperty(DataTransfer.prototype, "types", {
  configurable: true,
  enumerable: true,
  get() {
    const types = transferTypes.call(this);
    return carriesDroppedFiles(this) && !types.includes(URI_LIST) ? Object.freeze([...types, URI_LIST]) : types;
  },
});
DataTransfer.prototype.getData = function (format) {
  const data = getTransferData.call(this, format);
  if (data || String(format).toLowerCase() !== URI_LIST || !draggedTransfer(this, "drop") || !carriesDroppedFiles(this)) {
    return data;
  }
  return droppedPaths.map(fileUri).join("\r\n");
};
window.addEventListener("drop", (event) => {
  if (!carriesDroppedFiles(event.dataTransfer)) return;
  const unclaimed = [...droppedPaths];
  for (const file of event.dataTransfer.files) {
    const index = unclaimed.findIndex((path) => path.split(/[\\/]/).pop() === file.name);
    if (index < 0) continue;
    Object.defineProperty(file, "path", { value: unclaimed.splice(index, 1)[0], configurable: true });
  }
}, true);
if (commands.has("sabine.drag.files")) {
  const dragFiles = (files) => window.sabine.bridge.invoke("sabine.drag.files", { files }).catch(() => {});
  DataTransfer.prototype.setData = function (format, data) {
    setTransferData.call(this, format, data);
    if (String(format).toLowerCase() !== URI_LIST || !draggedTransfer(this, "dragstart")) return;
    const started = window.event;
    dragFiles(String(data).split(/\r?\n/).map((line) => line.trim()).filter((line) => line.startsWith("file:")));
    setTimeout(() => { if (started.defaultPrevented) dragFiles([]); });
  };
}
