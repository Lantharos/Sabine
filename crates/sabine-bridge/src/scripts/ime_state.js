(report) => {
  const reach = 1500;
  const lowSurrogate = /^[\uDC00-\uDFFF]/;
  const highSurrogate = /[\uD800-\uDBFF]$/;
  let current = null;
  let reported = "";
  let queued = false;
  let watching = false;

  const texts = (root) => document.createTreeWalker(root, NodeFilter.SHOW_TEXT);

  const textBefore = (root, node, offset) => {
    const walker = texts(root);
    let text = node;
    let end = offset;
    if (node.nodeType !== Node.TEXT_NODE) {
      const next = node.childNodes[offset];
      walker.currentNode = next ?? node;
      text = next ? walker.previousNode() : walker.lastChild() ?? walker.previousNode();
      end = text ? text.data.length : 0;
    } else {
      walker.currentNode = node;
    }
    let collected = "";
    let start = end;
    while (text) {
      start = Math.max(0, end - (reach - collected.length));
      collected = text.data.slice(start, end) + collected;
      const previous = collected.length < reach && walker.previousNode();
      if (!previous) break;
      text = previous;
      end = text.data.length;
    }
    if (lowSurrogate.test(collected)) {
      collected = collected.slice(1);
      start += 1;
    }
    return { text: collected, node: text, offset: start };
  };

  const textAfter = (root, node, offset) => {
    const walker = texts(root);
    let text = node;
    let start = offset;
    if (node.nodeType !== Node.TEXT_NODE) {
      const next = node.childNodes[offset];
      walker.currentNode = next ?? node;
      text = !next ? walker.nextSibling() : next.nodeType === Node.TEXT_NODE ? next : walker.nextNode();
      start = 0;
    } else {
      walker.currentNode = node;
    }
    const first = { node: text, offset: start };
    let collected = "";
    while (text && collected.length < reach) {
      collected += text.data.slice(start, start + reach - collected.length);
      text = walker.nextNode();
      start = 0;
    }
    if (highSurrogate.test(collected)) collected = collected.slice(0, -1);
    return { text: collected, ...first };
  };

  const advance = (root, node, offset, count) => {
    const walker = texts(root);
    walker.currentNode = node;
    for (let text = node, position = offset; text; text = walker.nextNode(), position = 0) {
      if (count <= text.data.length - position) return [text, position + count];
      count -= text.data.length - position;
    }
    return null;
  };

  const control = (element) => {
    const value = element.value;
    const anchor = element.selectionStart;
    const cursor = element.selectionEnd;
    let start = Math.max(0, Math.min(anchor, cursor) - reach);
    let end = Math.min(value.length, Math.max(Math.max(anchor, cursor) + reach, start + 2 * reach));
    if (start > 0 && lowSurrogate.test(value[start])) start--;
    if (end < value.length && lowSurrogate.test(value[end])) end--;
    return {
      text: value.slice(start, end), anchor: anchor - start, cursor: cursor - start, base: start,
      select: (from, to) => {
        element.setSelectionRange(start + from, start + to);
        return true;
      },
      remove: (from, to) => element.setRangeText("", start + from, start + to, "end"),
      element,
    };
  };

  const editable = (element, selection) => {
    const range = selection.getRangeAt(0);
    const selected = range.collapsed ? "" : range.toString();
    if (selected.length > 4 * reach) return null;
    const head = textBefore(element, range.startContainer, range.startOffset);
    const tail = textAfter(element, range.endContainer, range.endOffset);
    const origin = head.node ? head : tail;
    const from = head.text.length;
    const to = from + selected.length;
    const backward = !range.collapsed && selection.anchorNode === range.endContainer &&
      selection.anchorOffset === range.endOffset;
    const select = (start, end) => {
      const startAt = origin.node && advance(element, origin.node, origin.offset, start);
      const endAt = startAt && advance(element, startAt[0], startAt[1], end - start);
      if (!endAt) return false;
      const target = document.createRange();
      target.setStart(startAt[0], startAt[1]);
      target.setEnd(endAt[0], endAt[1]);
      selection.removeAllRanges();
      selection.addRange(target);
      return true;
    };
    return {
      text: head.text + selected + tail.text, anchor: backward ? to : from,
      cursor: backward ? from : to, base: 0, select,
      remove: () => getSelection().getRangeAt(0).deleteContents(),
      element,
    };
  };

  const focused = () => {
    const element = document.activeElement;
    if (element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement) {
      if (element.type === "password" || typeof element.selectionStart !== "number") return null;
      return control(element);
    }
    if (!element?.isContentEditable) return null;
    const selection = getSelection();
    if (!selection || selection.rangeCount === 0 || !element.contains(selection.anchorNode) ||
        !element.contains(selection.focusNode)) return null;
    return editable(element, selection);
  };

  const watch = (active) => {
    if (active === watching) return;
    watching = active;
    const listen = active ? "addEventListener" : "removeEventListener";
    document[listen]("input", queue, true);
    document[listen]("selectionchange", queue, true);
  };

  const snapshot = (always) => {
    queued = false;
    current = focused();
    watch(current !== null);
    const state = JSON.stringify(current
      ? { text: current.text, cursor: current.cursor, anchor: current.anchor, base: current.base }
      : { text: "", cursor: 0, anchor: 0, base: 0 });
    if (always || state !== reported) {
      reported = state;
      report(state);
    }
  };

  const queue = () => {
    if (queued) return;
    queued = true;
    queueMicrotask(() => snapshot(false));
  };

  window.__sabineImeDelete = (start, end) => {
    const state = current;
    if (!state || state.element !== document.activeElement) return;
    const from = start - state.base;
    const to = end - state.base;
    if (from < 0 || to < from || to > state.text.length || !state.select(from, to)) return;
    if (!document.execCommand("delete")) {
      state.remove(from, to);
      state.element.dispatchEvent(new InputEvent("input", { bubbles: true,
        inputType: "deleteContentBackward" }));
    }
    queue();
  };

  const refocus = () => queueMicrotask(() => snapshot(true));
  document.addEventListener("focusin", refocus, true);
  document.addEventListener("focusout", queue, true);
  snapshot(true);
}
