export interface AppSettings {
  threadCount: number | null;
  logFolder: string | null;
  maxRetries: number;
  recentFolders: string[];
  verboseLogging: boolean;
}
