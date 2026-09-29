// Connects a page to the desktop clipboard on Linux, where Chromium renders
// offscreen and cannot reach the compositor's selections itself. The browser
// host evaluates this file in every frame, calls the result with a private
// transport and whether the frame shows the app's own pages, and hands the
// function it returns each reply from the window.

((post, trusted) => {
  const { stringify, parse } = JSON;
  const pending = new Map();
  let nextRequest = 0;

  const request = (op, body) =>
    new Promise((resolve, reject) => {
      const id = ++nextRequest;
      pending.set(id, { resolve, reject });
      post(stringify({ ...body, id, op }));
    });
  const read = (selection, types) => request("read", { selection, types });
  const write = (selection, items) => request("write", { selection, items });

  const encode = async (type, value) => {
    if (typeof value === "string") return { type, text: value };
    const bytes =
      value instanceof Uint8Array
        ? value
        : new Uint8Array(value instanceof Blob ? await value.arrayBuffer() : value);
    return type.startsWith("text/")
      ? { type, text: new TextDecoder().decode(bytes) }
      : { type, base64: bytes.toBase64() };
  };
  const decode = (item) => item.text ?? Uint8Array.fromBase64(item.base64);
  const encodeClipboardItems = async (clipboardItems) => {
    const items = [];
    for (const item of clipboardItems) {
      for (const type of item.types) items.push(await encode(type, await item.getType(type)));
    }
    return items;
  };

  const textControl = () => {
    const element = document.activeElement;
    return element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement
      ? element
      : null;
  };
  const selectedText = () => {
    const control = textControl();
    if (!control) return getSelection()?.toString() ?? "";
    if (control.type === "password" || typeof control.selectionStart !== "number") return "";
    return control.value.slice(control.selectionStart, control.selectionEnd);
  };
  const selectedItems = () => {
    const text = selectedText();
    if (!text) return [];
    const items = [{ type: "text/plain", text }];
    const selection = getSelection();
    if (!textControl() && selection?.rangeCount) {
      const container = document.createElement("div");
      for (let index = 0; index < selection.rangeCount; index++) {
        container.append(selection.getRangeAt(index).cloneContents());
      }
      items.push({ type: "text/html", text: container.innerHTML });
    }
    return items;
  };

  const clipboard = navigator.clipboard;
  const native = clipboard && {
    read: clipboard.read.bind(clipboard),
    write: clipboard.write.bind(clipboard),
    writeText: clipboard.writeText.bind(clipboard),
  };

  const exportCopy = (event) => {
    if (trusted) {
      setTimeout(() => native.read().then(encodeClipboardItems).then((items) => write("clipboard", items)).catch(() => {}));
      return;
    }
    const fallback = selectedItems();
    let exported = false;
    const complete = () => {
      if (exported) return;
      exported = true;
      removeEventListener(event.type, complete);
      const transfer = event.clipboardData;
      const items = event.defaultPrevented
        ? transfer.types.filter((type) => type !== "Files").map((type) => ({ type, text: transfer.getData(type) }))
        : fallback;
      if (items.length) write("clipboard", items).catch(() => {});
    };
    addEventListener(event.type, complete);
    setTimeout(complete);
  };
  for (const type of ["copy", "cut"]) {
    addEventListener(type, (event) => event.isTrusted && exportCopy(event), true);
  }

  const exportPrimary = () => {
    const text = selectedText();
    if (text) write("primary", [{ type: "text/plain", text }]).catch(() => {});
  };
  const editable = () => {
    const control = textControl();
    return control ? !control.readOnly && !control.disabled : Boolean(document.activeElement?.isContentEditable);
  };
  const placeCaret = (event) => {
    const position = document.caretPositionFromPoint(event.clientX, event.clientY);
    const control = textControl();
    if (!position) return;
    if (control && position.offsetNode === control) control.setSelectionRange(position.offset, position.offset);
    else if (!control) getSelection()?.collapse(position.offsetNode, position.offset);
  };
  let middlePaste = false;
  let plainPaste = false;
  addEventListener("keydown", (event) => {
    if (event.isTrusted) plainPaste = event.code === "KeyV" && event.shiftKey && (event.ctrlKey || event.metaKey);
  }, true);
  addEventListener("keyup", (event) => {
    if (event.isTrusted && (event.shiftKey || event.key === "Shift" || ((event.ctrlKey || event.metaKey) && event.code === "KeyA"))) {
      setTimeout(exportPrimary);
    }
  }, true);

  const deliverPaste = (target, items, plain) => {
    const transfer = new DataTransfer();
    for (const item of items) {
      if (plain && item.type !== "text/plain") continue;
      if (item.text !== undefined) transfer.setData(item.type, item.text);
      else transfer.items.add(new File([decode(item)], `clipboard.${item.type.split("/")[1]}`, { type: item.type }));
    }
    const paste = new ClipboardEvent("paste", { clipboardData: transfer, bubbles: true, cancelable: true, composed: true });
    if (!target.dispatchEvent(paste)) return;
    const html = transfer.getData("text/html");
    const text = transfer.getData("text/plain");
    if (html && document.activeElement?.isContentEditable) document.execCommand("insertHTML", false, html);
    else if (text) document.execCommand("insertText", false, text);
  };
  addEventListener("paste", (event) => {
    if (!event.isTrusted) return;
    event.preventDefault();
    event.stopImmediatePropagation();
    if (middlePaste) return;
    const { target } = event;
    const plain = plainPaste;
    read("clipboard").then((items) => deliverPaste(target, items, plain), () => {});
  }, true);
  addEventListener("mouseup", (event) => {
    if (!event.isTrusted) return;
    if (event.button === 0) setTimeout(exportPrimary);
    if (event.button !== 1 || !editable()) return;
    middlePaste = true;
    setTimeout(() => { middlePaste = false; });
    placeCaret(event);
    const target = document.activeElement;
    read("primary", ["text/plain"]).then((items) => deliverPaste(target, items, true), () => {});
  }, true);

  if (clipboard) {
    Object.defineProperties(clipboard, {
      writeText: {
        value: async (text) => {
          await native.writeText(text);
          await write("clipboard", [{ type: "text/plain", text: String(text) }]);
        },
      },
      write: {
        value: async (clipboardItems) => {
          await native.write(clipboardItems);
          await write("clipboard", await encodeClipboardItems(clipboardItems));
        },
      },
    });
  }

  if (trusted) {
    const readRecord = async (selection, types) =>
      Object.fromEntries((await read(selection, types)).map((item) => [item.type, decode(item)]));
    if (clipboard) {
      Object.defineProperties(clipboard, {
        readText: { value: async () => (await readRecord("clipboard", ["text/plain"]))["text/plain"] ?? "" },
        read: {
          value: async () => {
            const items = await read("clipboard");
            if (!items.length) return [];
            return [new ClipboardItem(Object.fromEntries(items.map((item) => [item.type, new Blob([decode(item)], { type: item.type })])))];
          },
        },
      });
    }
    window.sabine = window.sabine || {};
    window.sabine.clipboard = Object.freeze({
      read: ({ selection = "clipboard", types } = {}) => readRecord(selection, types),
      write: async (data, { selection = "clipboard" } = {}) =>
        write(selection, await Promise.all(Object.entries(data).map(([type, value]) => encode(type, value)))),
    });
  }

  return (message) => {
    const reply = parse(message);
    const entry = pending.get(reply.id);
    if (!entry) return;
    pending.delete(reply.id);
    if (reply.ok) entry.resolve(reply.value);
    else entry.reject(new DOMException(reply.value?.message ?? "The clipboard is unavailable", "NotAllowedError"));
  };
})
