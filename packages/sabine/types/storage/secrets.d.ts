/**
 * Text the app keeps in the system's credential store, such as tokens: the
 * Secret Service on Linux, the login keychain on macOS and the Credential
 * Manager on Windows. Calls can wait while the system asks the person to
 * unlock it.
 */
export declare const secrets: {
  /** The value under `key`, or `null` when there is none. */
  get(key: string): Promise<string | null>;
  set(key: string, value: string): Promise<void>;
  /** Resolves to whether there was a value under `key`. */
  delete(key: string): Promise<boolean>;
};
