(() => {
  document.documentElement?.style.setProperty("background", "transparent");
  document.body?.style.setProperty("background", "transparent");
  if (document.querySelector("style[data-sabine-transparent-background]")) return;
  const style = document.createElement("style");
  style.setAttribute("data-sabine-transparent-background", "");
  style.textContent = "html,body{background:transparent!important;}";
  document.head?.appendChild(style);
})();
