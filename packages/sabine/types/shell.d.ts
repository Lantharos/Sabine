/**
 * Handing links and folders to the person's own apps: the desktop portal on
 * Linux, which also works inside a sandbox, and the system's shell on macOS
 * and Windows.
 */
export declare const shell: {
  /** Opens an `http:`, `https:` or `mailto:` link in the default browser or mail app. */
  openLink(url: string): Promise<void>;
  /** Opens a folder in the file manager. */
  showFolder(path: string): Promise<void>;
};
