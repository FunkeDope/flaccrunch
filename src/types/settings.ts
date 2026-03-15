export type Theme = "light" | "dark" | "system";

export interface AppSettings {
  threadCount: number | null;
  logFolder: string | null;
  maxRetries: number;
  recentFolders: string[];
  theme: Theme;
}
