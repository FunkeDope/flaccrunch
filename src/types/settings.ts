export interface AppSettings {
  threadCount: number | null;
  logFolder: string | null;
  maxRetries: number;
  verboseLogging: boolean;
  markAsCrunched: boolean;
  skipCrunched: boolean;
}
