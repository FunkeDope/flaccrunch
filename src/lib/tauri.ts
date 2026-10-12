import { invoke } from "@tauri-apps/api/core";
import type { ScanResult, ProcessingSettings } from "../types/processing";
import type { AppSettings } from "../types/settings";

// Folder/file selection
export async function selectFolders(): Promise<string[]> {
  return invoke<string[]>("select_folders");
}

export async function selectFiles(): Promise<string[]> {
  return invoke<string[]>("select_files");
}

export async function isMobile(): Promise<boolean> {
  return invoke<boolean>("is_mobile");
}

export async function scanFolders(folders: string[]): Promise<ScanResult> {
  return invoke<ScanResult>("scan_folders", { folders });
}

// Processing operations
export async function startProcessing(
  folders: string[],
  settings: ProcessingSettings
): Promise<string> {
  return invoke<string>("start_processing", { folders, settings });
}

export async function cancelProcessing(): Promise<void> {
  return invoke<void>("cancel_processing");
}

export async function enqueueFiles(paths: string[]): Promise<number> {
  return invoke<number>("enqueue_files", { paths });
}

// Settings
export async function getSettings(): Promise<AppSettings> {
  return invoke<AppSettings>("get_settings");
}

export async function saveSettings(settings: AppSettings): Promise<void> {
  return invoke<void>("save_settings", { settings });
}

export async function getSettingsPath(): Promise<string> {
  return invoke<string>("get_settings_path");
}

export async function getCpuCount(): Promise<number> {
  return invoke<number>("get_cpu_count");
}

export async function getDefaultLogFolder(): Promise<string> {
  return invoke<string>("get_default_log_folder");
}

// Logs
export async function getEfcLog(
  events: unknown[],
  elapsedSecs: number,
  sourceFolder: string,
  startMs: number,
  finishMs: number,
  threadCount: number,
  maxRetries: number,
  runCanceled: boolean,
): Promise<string> {
  return invoke<string>("get_efc_log", {
    events,
    elapsedSecs,
    sourceFolder,
    startMs,
    finishMs,
    threadCount,
    maxRetries,
    runCanceled,
  });
}

export async function writeTextFile(path: string, content: string): Promise<void> {
  return invoke<void>("write_text_file", { path, content });
}

// Startup paths from CLI args (consumed on first call)
export async function getStartupPaths(): Promise<string[]> {
  return invoke<string[]>("get_startup_paths");
}

export async function copyFileToPath(sourcePath: string, destinationPath: string): Promise<void> {
  return invoke<void>("copy_file_to_path", { sourcePath, destinationPath });
}

// Menu bar: About, Dev menu, folders and project links
export type ExternalTarget = "settings-folder" | "log-folder" | "project" | "releases" | "issues";

export async function getLibflacVersion(): Promise<string> {
  return invoke<string>("get_libflac_version");
}

export async function toggleDevtools(): Promise<void> {
  return invoke<void>("toggle_devtools");
}

/** Opens a known folder in the file manager or a project page in the browser. */
export async function openExternal(target: ExternalTarget): Promise<void> {
  return invoke<void>("open_external", { target });
}
