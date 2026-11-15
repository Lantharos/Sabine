import { createSignal } from "solid-js";
import { commands } from "./bindings";

export function App() {
  const [status, setStatus] = createSignal("Ready.");

  async function showVersion() {
    try {
      const { version } = await commands.appVersion();
      setStatus(`Version ${version}`);
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error));
    }
  }

  return (
    <main>
      <h1>{document.title}</h1>
      <p>{status()}</p>
      <button type="button" onClick={showVersion}>
        App version
      </button>
    </main>
  );
}
