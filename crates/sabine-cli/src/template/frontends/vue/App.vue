<script setup lang="ts">
import { ref } from "vue";
import { commands } from "./bindings";

const title = document.title;
const status = ref("Ready.");

async function showVersion() {
  try {
    const { version } = await commands.appVersion();
    status.value = `Version ${version}`;
  } catch (error) {
    status.value = error instanceof Error ? error.message : String(error);
  }
}
</script>

<template>
  <main>
    <h1>{{ title }}</h1>
    <p>{{ status }}</p>
    <button type="button" @click="showVersion">App version</button>
  </main>
</template>
