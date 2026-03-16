import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock the Tauri core invoke before importing the module under test
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";
import {
  selectFolders,
  selectFiles,
  isMobile,
  scanFolders,
  validateFolder,
  startProcessing,
  cancelProcessing,
  getSettings,
  saveSettings,
  getCpuCount,
  getDefaultLogFolder,
  getEfcLog,
  getStartupPaths,
} from "./tauri";

const mockInvoke = vi.mocked(invoke);

beforeEach(() => {
  mockInvoke.mockReset();
});

describe("tauri wrappers — command names", () => {
  it("selectFolders calls 'select_folders'", async () => {
    mockInvoke.mockResolvedValue([]);
    await selectFolders();
    expect(mockInvoke).toHaveBeenCalledWith("select_folders");
  });

  it("selectFiles calls 'select_files'", async () => {
    mockInvoke.mockResolvedValue([]);
    await selectFiles();
    expect(mockInvoke).toHaveBeenCalledWith("select_files");
  });

  it("isMobile calls 'is_mobile'", async () => {
    mockInvoke.mockResolvedValue(false);
    await isMobile();
    expect(mockInvoke).toHaveBeenCalledWith("is_mobile");
  });

  it("scanFolders calls 'scan_folders' with folders argument", async () => {
    mockInvoke.mockResolvedValue({ files: [], totalSize: 0 });
    await scanFolders(["/music"]);
    expect(mockInvoke).toHaveBeenCalledWith("scan_folders", { folders: ["/music"] });
  });

  it("validateFolder calls 'validate_folder' with path argument", async () => {
    mockInvoke.mockResolvedValue(true);
    await validateFolder("/music");
    expect(mockInvoke).toHaveBeenCalledWith("validate_folder", { path: "/music" });
  });

  it("startProcessing calls 'start_processing' with folders and settings", async () => {
    mockInvoke.mockResolvedValue("ok");
    const settings = { threadCount: 2, logFolder: "/logs", maxRetries: 3, verboseLogging: false };
    await startProcessing(["/music"], settings);
    expect(mockInvoke).toHaveBeenCalledWith("start_processing", {
      folders: ["/music"],
      settings,
    });
  });

  it("cancelProcessing calls 'cancel_processing'", async () => {
    mockInvoke.mockResolvedValue(undefined);
    await cancelProcessing();
    expect(mockInvoke).toHaveBeenCalledWith("cancel_processing");
  });

  it("getSettings calls 'get_settings'", async () => {
    mockInvoke.mockResolvedValue({ maxRetries: 3, recentFolders: [] });
    await getSettings();
    expect(mockInvoke).toHaveBeenCalledWith("get_settings");
  });

  it("saveSettings calls 'save_settings' with settings argument", async () => {
    mockInvoke.mockResolvedValue(undefined);
    const settings = { threadCount: null, logFolder: null, maxRetries: 3, recentFolders: [], verboseLogging: false };
    await saveSettings(settings);
    expect(mockInvoke).toHaveBeenCalledWith("save_settings", { settings });
  });

  it("getCpuCount calls 'get_cpu_count'", async () => {
    mockInvoke.mockResolvedValue(8);
    const result = await getCpuCount();
    expect(result).toBe(8);
    expect(mockInvoke).toHaveBeenCalledWith("get_cpu_count");
  });

  it("getDefaultLogFolder calls 'get_default_log_folder'", async () => {
    mockInvoke.mockResolvedValue("/var/log/flaccrunch");
    await getDefaultLogFolder();
    expect(mockInvoke).toHaveBeenCalledWith("get_default_log_folder");
  });

  it("getEfcLog calls 'get_efc_log' with events and elapsedSecs", async () => {
    mockInvoke.mockResolvedValue("log text");
    await getEfcLog([], 42);
    expect(mockInvoke).toHaveBeenCalledWith("get_efc_log", { events: [], elapsedSecs: 42 });
  });

  it("getStartupPaths calls 'get_startup_paths'", async () => {
    mockInvoke.mockResolvedValue([]);
    await getStartupPaths();
    expect(mockInvoke).toHaveBeenCalledWith("get_startup_paths");
  });
});

describe("tauri wrappers — return values are forwarded", () => {
  it("scanFolders returns the resolved ScanResult", async () => {
    const result = { files: ["/a.flac", "/b.flac"], totalSize: 2048 };
    mockInvoke.mockResolvedValue(result);
    const got = await scanFolders(["/music"]);
    expect(got).toEqual(result);
  });

  it("getCpuCount returns the number", async () => {
    mockInvoke.mockResolvedValue(16);
    expect(await getCpuCount()).toBe(16);
  });

  it("validateFolder returns boolean", async () => {
    mockInvoke.mockResolvedValue(true);
    expect(await validateFolder("/music")).toBe(true);
  });
});
