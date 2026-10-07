export interface ActivityOptions {
  /** Shown in diagnostics; up to 128 characters. Defaults to `activity`. */
  name?: string;
  /**
   * Keeps the page running while its window is out of sight, instead of letting Sabine freeze it.
   * Defaults to `true`.
   */
  keepRunning?: boolean;
}

export interface ActivityRecord {
  id: string;
  name: string;
  keepRunning: boolean;
}

export interface ActivityEnd {
  id: string;
  /** False when the activity had already ended. */
  ended: boolean;
}

export interface ActivityHandle extends ActivityRecord {
  end(): Promise<ActivityEnd>;
}

export interface ActivityList {
  activities: ActivityRecord[];
  /** How many running activities keep the page running while its window is out of sight. */
  keepingRunning: number;
}

export interface SabineActivityApi {
  begin(options?: ActivityOptions): Promise<ActivityHandle>;
  list(): Promise<ActivityList>;
}

export declare const activity: SabineActivityApi;
