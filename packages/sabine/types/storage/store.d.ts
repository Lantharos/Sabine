/** A key of the store that changed, from any of the app's pages or its Rust side. */
export interface StoreChangedEvent {
  key: string;
  /** The key's new value, or `null` once it was deleted. */
  value: unknown;
}

/**
 * The app's JSON values by key, kept in its data folder and shared with its
 * Rust side and every window. Changes reach `events.storeChanged`.
 */
export declare const store: {
  /** The value under `key`, or `undefined` when there is none. */
  get<T = unknown>(key: string): Promise<T | undefined>;
  set(key: string, value: unknown): Promise<void>;
  /** Resolves to whether the store held `key`. */
  delete(key: string): Promise<boolean>;
  entries(): Promise<Record<string, unknown>>;
  clear(): Promise<void>;
};
