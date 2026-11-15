/** A download the window's page started, as it stands. */
export interface DownloadEvent {
  downloadId: string;
  url: string;
  filename: string;
  mimeType: string;
  /** `0` or less while the size is unknown. */
  totalBytes: number;
  receivedBytes: number;
  state: "requested" | "progress" | "completed" | "cancelled" | "interrupted";
  savePath: string;
  /**
   * Why an `interrupted` download stopped, such as `networkFailed`,
   * `fileNoSpace` or `serverForbidden`.
   */
  error?: string;
}

/**
 * The downloads the window's page starts. They go where the app's Rust code
 * decides, or where people choose in the desktop's save dialog.
 */
export interface SabineDownloadsApi {
  pause(downloadId: string): Promise<void>;
  resume(downloadId: string): Promise<void>;
  cancel(downloadId: string): Promise<void>;
}

export declare const downloads: SabineDownloadsApi;
