export interface ActivityOptions {
  /** Shown in diagnostics; up to 128 characters. Defaults to `activity`. */
  name?: string;
  /** Keeps the window from hibernating while the activity runs. Defaults to `true`. */
  preventsHibernation?: boolean;
}

export interface ActivityRecord {
  id: string;
  name: string;
  preventsHibernation: boolean;
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
  /** How many running activities keep the window from hibernating. */
  hibernationBlockers: number;
}

export interface SabineActivityApi {
  begin(options?: ActivityOptions): Promise<ActivityHandle>;
  list(): Promise<ActivityList>;
}

export declare const activity: SabineActivityApi;
