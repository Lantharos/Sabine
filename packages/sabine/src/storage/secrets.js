import { invoke } from "../bridge.js";

/** @type {typeof import("../../types/storage/secrets.js").secrets} */
export const secrets = {
  get(key) {
    return invoke("sabine.secrets.get", { key: String(key) });
  },
  async set(key, value) {
    await invoke("sabine.secrets.set", { key: String(key), value: String(value) });
  },
  delete(key) {
    return invoke("sabine.secrets.delete", { key: String(key) });
  },
};
