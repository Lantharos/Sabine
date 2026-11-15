import { invoke } from "../bridge.js";

/** @type {typeof import("../../types/storage/store.js").store} */
export const store = {
  async get(key) {
    return (await invoke("sabine.store.get", { key: String(key) })) ?? undefined;
  },
  async set(key, value) {
    await invoke("sabine.store.set", { key: String(key), value });
  },
  delete(key) {
    return invoke("sabine.store.delete", { key: String(key) });
  },
  entries() {
    return invoke("sabine.store.entries");
  },
  async clear() {
    await invoke("sabine.store.clear");
  },
};
