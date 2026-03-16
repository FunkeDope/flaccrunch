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

export async function selectOutputFolder(): Promise<string | null> {
  return invoke<string | null>("select_output_folder");
}

export async function scanFolders(folders: string[]): Promise<ScanResult> {
  return invoke<ScanResult>("scan_folders", { folders });
}

export async function validateFolder(path: string): Promise<boolean> {
  return invoke<boolean>("validate_folder", { path });
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

// Settings
export async function getSettings(): Promise<AppSettings> {
  return invoke<AppSettings>("get_settings");
}

export async function saveSettings(settings: AppSettings): Promise<void> {
  return invoke<void>("save_settings", { settings });
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

// Startup paths from CLI args (consumed on first call)
export async function getStartupPaths(): Promise<string[]> {
  return invoke<string[]>("get_startup_paths");
}
