export interface PrintFilesOptions {
  /** The name of the print job. Defaults to the first file's name. */
  title?: string;
}

export interface SabinePrintApi {
  page(): Promise<void>;
  element(element: Element): Promise<void>;
  files(paths: Iterable<string>, options?: PrintFilesOptions): Promise<boolean>;
}

/**
 * Printing through the desktop's print dialog: the print portal on Linux and
 * the system's own dialog on macOS and Windows.
 */
export declare const print: {
  /** Prints the page, like `window.print()`. */
  page(): Promise<void>;
  /**
   * Prints one element of the page on its own. Images, canvases and videos
   * are scaled to fit the paper.
   */
  element(element: Element): Promise<void>;
  /**
   * Prints PDF, image and text files as one document. Resolves `false` when
   * the person cancels the print dialog on Linux.
   */
  files(paths: Iterable<string>, options?: PrintFilesOptions): Promise<boolean>;
};
