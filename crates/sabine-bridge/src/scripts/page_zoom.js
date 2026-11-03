(report) => {
  const watch = () => {
    matchMedia(`(resolution: ${devicePixelRatio}dppx)`).addEventListener("change", () => {
      report(String(devicePixelRatio));
      watch();
    }, { once: true });
  };
  report(String(devicePixelRatio));
  watch();
}
