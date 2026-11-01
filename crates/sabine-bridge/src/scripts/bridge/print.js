// `window.sabine.print`: the page, one of its elements, or files through the
// desktop's print dialog.

const PRINT_PATH = "data-sabine-print-path";
const PRINT_TARGET = "data-sabine-print-target";
const printElementStyle = `@media print {
  [${PRINT_PATH}] > :not([${PRINT_PATH}], [${PRINT_TARGET}]) { display: none !important; }
  [${PRINT_PATH}] {
    display: block !important; position: static !important; inset: auto !important;
    width: auto !important; height: auto !important; min-width: 0 !important; min-height: 0 !important;
    max-width: none !important; max-height: none !important; margin: 0 !important; padding: 0 !important;
    border: 0 !important; overflow: visible !important; transform: none !important;
    background: none !important; box-shadow: none !important;
  }
  [${PRINT_TARGET}] { position: static !important; visibility: visible !important; transform: none !important; margin: 0 auto !important; }
  :is(img, canvas, svg, video)[${PRINT_TARGET}] {
    display: block !important; width: auto !important; height: auto !important;
    max-width: 100% !important; max-height: 100vh !important; object-fit: contain !important;
  }
}`;
window.sabine.print = {
  async page() {
    window.print();
  },
  async element(element) {
    const path = [];
    for (let node = element.parentElement; node; node = node.parentElement) {
      node.setAttribute(PRINT_PATH, "");
      path.push(node);
    }
    element.setAttribute(PRINT_TARGET, "");
    const style = document.createElement("style");
    style.textContent = printElementStyle;
    document.head.append(style);
    try {
      window.print();
    } finally {
      style.remove();
      element.removeAttribute(PRINT_TARGET);
      for (const node of path) node.removeAttribute(PRINT_PATH);
    }
  },
  files(paths, { title } = {}) {
    return window.sabine.bridge.invoke("sabine.print.files", { paths: Array.from(paths, String), title }, { timeoutMs: 2147483647 });
  },
};
