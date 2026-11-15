// Connects a page to the desktop clipboard through the window. On Linux,
// where Chromium renders offscreen and cannot reach the compositor's
// selections itself, it carries every copy and paste. Elsewhere Chromium
// copies and pastes by itself, and the app's own pages gain
// `window.sabine.clipboard` for any other type. The browser host evaluates
// this file in every frame, calls the result with a private transport and
// whether the frame shows the app's own pages, and hands the function it
// returns each reply from the window. Each item's bytes travel together,
// one after another, beside a list of the items' types and sizes.

((post, trusted) => {
  const { stringify, parse } = JSON;
  const encoder = new TextEncoder();
  const decoder = new TextDecoder();
  const pending = new Map();
  let nextRequest = 0;

  const settle = (id) => {
    const entry = pending.get(id);
    pending.delete(id);
    clearTimeout(entry?.timer);
    return entry;
  };
  const request = (op, fields, bytes) =>
    new Promise((resolve, reject) => {
      const id = ++nextRequest;
      const timer = setTimeout(() => {
        settle(id);
        reject(new DOMException("The clipboard did not answer", "TimeoutError"));
      }, 30000);
      pending.set(id, { resolve, reject, timer });
      try {
        const message = stringify({ ...fields, id, op });
        if (bytes) post(message, bytes);
        else post(message);
      } catch (error) {
        settle(id);
        reject(error);
      }
    });
  addEventListener("pagehide", () => {
    for (const id of [...pending.keys()]) {
      settle(id).reject(new DOMException("The page was hidden", "AbortError"));
    }
  });
  const unpack = (items, body) => {
    let offset = 0;
    return items.map(({ type, size }) => {
      const bytes = new Uint8Array(body, offset, size);
      offset += size;
      return { type, value: type.startsWith("text/") ? decoder.decode(bytes) : bytes };
    });
  };
  const pack = (entries) => {
    const body = new Uint8Array(entries.reduce((size, [, bytes]) => size + bytes.byteLength, 0));
    let offset = 0;
    const items = entries.map(([type, bytes]) => {
      body.set(bytes, offset);
      offset += bytes.byteLength;
      return { type, size: bytes.byteLength };
    });
    return { items, body: body.buffer };
  };
  const read = (selection, types) =>
    request("read", { selection, types }).then(({ value, body }) => unpack(value, body));
  const write = (selection, entries) => {
    const { items, body } = pack(entries.map(([type, value]) => [type, typeof value === "string" ? encoder.encode(value) : value]));
    return request("write", { selection, items }, body).then(({ value }) => value);
  };

  const bytesOf = async (value) => {
    if (typeof value === "string") return encoder.encode(value);
    if (value instanceof Blob) return new Uint8Array(await value.arrayBuffer());
    if (ArrayBuffer.isView(value)) return new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
    return new Uint8Array(value);
  };
  const encodeClipboardItems = async (clipboardItems) => {
    const entries = [];
    for (const item of clipboardItems) {
      for (const type of item.types) entries.push([type, await bytesOf(await item.getType(type))]);
    }
    return entries;
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
    const entries = [["text/plain", text]];
    const selection = getSelection();
    if (!textControl() && selection?.rangeCount) {
      const container = document.createElement("div");
      for (let index = 0; index < selection.rangeCount; index++) {
        container.append(selection.getRangeAt(index).cloneContents());
      }
      entries.push(["text/html", container.innerHTML]);
    }
    return entries;
  };

  const carriesCopyAndPaste = navigator.platform.startsWith("Linux");
  const clipboard = carriesCopyAndPaste ? navigator.clipboard : null;
  const native = clipboard && {
    read: clipboard.read.bind(clipboard),
    write: clipboard.write.bind(clipboard),
    writeText: clipboard.writeText.bind(clipboard),
  };

  if (carriesCopyAndPaste) {
    const exportCopy = (event) => {
      if (trusted) {
        setTimeout(() => native.read().then(encodeClipboardItems).then((entries) => write("clipboard", entries)).catch(() => {}));
        return;
      }
      const fallback = selectedItems();
      let exported = false;
      const complete = () => {
        if (exported) return;
        exported = true;
        removeEventListener(event.type, complete);
        const transfer = event.clipboardData;
        const entries = event.defaultPrevented
          ? transfer.types.filter((type) => type !== "Files").map((type) => [type, transfer.getData(type)])
          : fallback;
        if (entries.length) write("clipboard", entries).catch(() => {});
      };
      addEventListener(event.type, complete);
      setTimeout(complete);
    };
    for (const type of ["copy", "cut"]) {
      addEventListener(type, (event) => event.isTrusted && exportCopy(event), true);
    }

    const exportPrimary = () => {
      const text = selectedText();
      if (text) write("primary", [["text/plain", text]]).catch(() => {});
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
        if (typeof item.value === "string") transfer.setData(item.type, item.value);
        else transfer.items.add(new File([item.value], `clipboard.${item.type.split("/")[1]}`, { type: item.type }));
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
  }

  if (clipboard) {
    Object.defineProperties(clipboard, {
      writeText: {
        value: async (text) => {
          await native.writeText(text);
          await write("clipboard", [["text/plain", String(text)]]);
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
      Object.fromEntries((await read(selection, types)).map((item) => [item.type, item.value]));
    if (clipboard) {
      Object.defineProperties(clipboard, {
        readText: { value: async () => (await readRecord("clipboard", ["text/plain"]))["text/plain"] ?? "" },
        read: {
          value: async () => {
            const items = await read("clipboard");
            if (!items.length) return [];
            return [new ClipboardItem(Object.fromEntries(items.map((item) => [item.type, new Blob([item.value], { type: item.type })])))];
          },
        },
      });
    }
    window.sabine = window.sabine || {};
    window.sabine.clipboard = Object.freeze({
      read: ({ selection = "clipboard", types } = {}) => readRecord(selection, types),
      write: async (data, { selection = "clipboard" } = {}) =>
        write(selection, await Promise.all(Object.entries(data).map(async ([type, value]) => [type, await bytesOf(value)]))),
    });
  }

  return (message, body) => {
    const reply = parse(message);
    const entry = settle(reply.id);
    if (!entry) return;
    if (reply.ok) entry.resolve({ value: reply.value, body });
    else entry.reject(new DOMException(reply.value?.message ?? "The clipboard is unavailable", "NotAllowedError"));
  };
})
