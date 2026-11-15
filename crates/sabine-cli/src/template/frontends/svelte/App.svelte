<script lang="ts">
  import { commands } from "./bindings";

  let status = $state("Ready.");

  async function showVersion() {
    try {
      const { version } = await commands.appVersion();
      status = `Version ${version}`;
    } catch (error) {
      status = error instanceof Error ? error.message : String(error);
    }
  }
</script>

<main>
  <h1>{document.title}</h1>
  <p>{status}</p>
  <button type="button" onclick={showVersion}>App version</button>
</main>
