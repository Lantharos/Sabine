export type ClipboardSelection = "clipboard" | "primary";

/** Clipboard contents by MIME type: text types as strings, others as bytes. */
export type ClipboardData = Record<string, string | Uint8Array>;

export interface ClipboardReadOptions {
  /** `primary` is the selection that middle-click pastes, on Linux only. */
  selection?: ClipboardSelection;
  /** MIME types to read. Defaults to plain text, HTML, file lists and one image. */
  types?: string[];
}

export interface ClipboardWriteOptions {
  selection?: ClipboardSelection;
}

export interface SabineClipboardApi {
  read(options?: ClipboardReadOptions): Promise<ClipboardData>;
  write(
    data: Record<string, string | Uint8Array | Blob>,
    options?: ClipboardWriteOptions,
  ): Promise<void>;
}

/**
 * The desktop clipboard with any MIME type, and the primary selection on
 * Linux. Elsewhere it uses the browser clipboard.
 */
export declare const clipboard: SabineClipboardApi;
