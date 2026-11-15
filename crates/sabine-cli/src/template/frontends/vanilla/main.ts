import { commands } from "./bindings";
import "./style.css";

const status = document.querySelector("#status");
const button = document.querySelector("#version");

if (status instanceof HTMLElement) {
  status.textContent = "Ready.";
}

button?.addEventListener("click", async () => {
  try {
    const { version } = await commands.appVersion();
    if (status instanceof HTMLElement) {
      status.textContent = `Version ${version}`;
    }
  } catch (error) {
    if (status instanceof HTMLElement) {
      status.textContent = error instanceof Error ? error.message : String(error);
    }
  }
});
