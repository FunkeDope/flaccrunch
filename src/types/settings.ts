export interface AppSettings {
  threadCount: number | null;
  logFolder: string | null;
  maxRetries: number;
  verboseLogging: boolean;
  markAsCrunched: boolean;
  skipCrunched: boolean;
  /** Non-subset maximum compression (smaller; some old hardware players reject it). */
  maxCompression: boolean;
}
