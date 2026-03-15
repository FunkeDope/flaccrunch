import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook, waitFor, act } from "@testing-library/react";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../lib/tauri", () => ({
  getCpuCount: vi.fn(),
  getDefaultLogFolder: vi.fn(),
  getSettings: vi.fn(),
  saveSettings: vi.fn(),
}));

import * as api from "../lib/tauri";
import { useSettings } from "./useSettings";

const mockGetCpuCount = vi.mocked(api.getCpuCount);
const mockGetDefaultLogFolder = vi.mocked(api.getDefaultLogFolder);
const mockGetSettings = vi.mocked(api.getSettings);
const mockSaveSettings = vi.mocked(api.saveSettings);

beforeEach(() => {
  vi.clearAllMocks();
  mockGetCpuCount.mockResolvedValue(8);
  mockGetDefaultLogFolder.mockResolvedValue("/var/log");
  mockGetSettings.mockResolvedValue({
    threadCount: null,
    logFolder: null,
    maxRetries: 3,
    recentFolders: [],
  });
  mockSaveSettings.mockResolvedValue(undefined);
});

describe("useSettings — initial state", () => {
  it("starts with default settings before API resolves", () => {
    const { result } = renderHook(() => useSettings());
    expect(result.current.settings.maxRetries).toBe(3);
    expect(result.current.settings.threadCount).toBeNull();
  });

  it("loads cpuCount from API", async () => {
    const { result } = renderHook(() => useSettings());
    await waitFor(() => {
      expect(result.current.cpuCount).toBe(8);
    });
  });
});

describe("useSettings — processingSettings derivation", () => {
  it("derives threadCount as cpuCount - 1 when threadCount is null", async () => {
    const { result } = renderHook(() => useSettings());
    await waitFor(() => expect(result.current.cpuCount).toBe(8));
    // cpuCount=8, threadCount=null → Math.max(1, 8-1) = 7
    expect(result.current.processingSettings.threadCount).toBe(7);
  });

  it("uses explicit threadCount when set", async () => {
    mockGetSettings.mockResolvedValue({
      threadCount: 4,
      logFolder: null,
      maxRetries: 3,
      recentFolders: [],
    });
    const { result } = renderHook(() => useSettings());
    await waitFor(() => expect(result.current.settings.threadCount).toBe(4));
    expect(result.current.processingSettings.threadCount).toBe(4);
  });

  it("uses default log folder when logFolder is null", async () => {
    const { result } = renderHook(() => useSettings());
    await waitFor(() => expect(result.current.processingSettings.logFolder).toBe("/var/log"));
  });

  it("uses explicit logFolder when set", async () => {
    mockGetSettings.mockResolvedValue({
      threadCount: null,
      logFolder: "/custom/logs",
      maxRetries: 3,
      recentFolders: [],
    });
    const { result } = renderHook(() => useSettings());
    await waitFor(() =>
      expect(result.current.processingSettings.logFolder).toBe("/custom/logs")
    );
  });

  it("uses maxRetries from settings", async () => {
    mockGetSettings.mockResolvedValue({
      threadCount: null,
      logFolder: null,
      maxRetries: 5,
      recentFolders: [],
    });
    const { result } = renderHook(() => useSettings());
    await waitFor(() => expect(result.current.processingSettings.maxRetries).toBe(5));
  });

  it("clamps threadCount to at least 1 when cpuCount is 1", async () => {
    mockGetCpuCount.mockResolvedValue(1);
    const { result } = renderHook(() => useSettings());
    await waitFor(() => expect(result.current.cpuCount).toBe(1));
    // Math.max(1, 1-1) = Math.max(1, 0) = 1
    expect(result.current.processingSettings.threadCount).toBe(1);
  });
});

describe("useSettings — updateSettings", () => {
  it("calls saveSettings with merged settings", async () => {
    const { result } = renderHook(() => useSettings());
    await waitFor(() => expect(result.current.cpuCount).toBe(8));

    await result.current.updateSettings({ maxRetries: 5 });

    expect(mockSaveSettings).toHaveBeenCalledWith(
      expect.objectContaining({ maxRetries: 5 })
    );
  });

  it("updates local settings state", async () => {
    const { result } = renderHook(() => useSettings());
    await waitFor(() => expect(result.current.cpuCount).toBe(8));

    await act(async () => {
      await result.current.updateSettings({ maxRetries: 4 });
    });

    await waitFor(() => expect(result.current.settings.maxRetries).toBe(4));
  });
});
