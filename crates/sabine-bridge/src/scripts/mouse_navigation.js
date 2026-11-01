(x, y, button, modifiers) => {
  const target = document.elementFromPoint(x, y) || window;
  const init = {
    bubbles: true,
    cancelable: true,
    button,
    buttons: 0,
    clientX: x,
    clientY: y,
    screenX: x,
    screenY: y,
    shiftKey: (modifiers & (1 << 1)) !== 0,
    ctrlKey: (modifiers & (1 << 2)) !== 0,
    altKey: (modifiers & (1 << 3)) !== 0,
    metaKey: (modifiers & (1 << 7)) !== 0,
  };
  const up = new MouseEvent("mouseup", init);
  const aux = new MouseEvent("auxclick", init);
  const canceled = !target.dispatchEvent(up) || up.defaultPrevented ||
    !target.dispatchEvent(aux) || aux.defaultPrevented;
  if (canceled) return;
  if (button === 3) history.back();
  else if (button === 4) history.forward();
}
