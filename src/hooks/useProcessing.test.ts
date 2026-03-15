import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook, act, waitFor } from "@testing-library/react";

// Mock all Tauri dependencies
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(vi.fn())) }));
vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: vi.fn(() => ({
    onDragDropEvent: vi.fn(() => Promise.resolve(vi.fn())),
  })),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  save: vi.fn(() => Promise.resolve(null)),
}));
vi.mock("../lib/tauri", () => ({
  getStartupPaths: vi.fn(),
  selectFolders: vi.fn(),
  selectFiles: vi.fn(),
  scanFolders: vi.fn(),
  startProcessing: vi.fn(),
  cancelProcessing: vi.fn(),
}));

import * as api from "../lib/tauri";
import { useProcessing } from "./useProcessing";

const mockGetStartupPaths = vi.mocked(api.getStartupPaths);
const mockSelectFolders = vi.mocked(api.selectFolders);
const mockSelectFiles = vi.mocked(api.selectFiles);
const mockScanFolders = vi.mocked(api.scanFolders);
const mockStartProcessing = vi.mocked(api.startProcessing);
const mockCancelProcessing = vi.mocked(api.cancelProcessing);

beforeEach(() => {
  vi.clearAllMocks();
  mockGetStartupPaths.mockResolvedValue([]);
  mockSelectFolders.mockResolvedValue([]);
  mockSelectFiles.mockResolvedValue([]);
  mockScanFolders.mockResolvedValue({ files: [], totalSize: 0 });
  mockStartProcessing.mockResolvedValue("ok");
  mockCancelProcessing.mockResolvedValue(undefined);
});

describe("useProcessing — initial state", () => {
  it("starts in idle status", () => {
    const { result } = renderHook(() => useProcessing());
    expect(result.current.status).toBe("idle");
  });

  it("starts with empty folders", () => {
    const { result } = renderHook(() => useProcessing());
    expect(result.current.folders).toEqual([]);
  });

  it("starts with empty workers", () => {
    const { result } = renderHook(() => useProcessing());
    expect(result.current.workers).toEqual([]);
  });

  it("starts with no error", () => {
    const { result } = renderHook(() => useProcessing());
    expect(result.current.error).toBeNull();
  });
});

describe("useProcessing — startup paths", () => {
  it("loads startup paths from CLI on mount", async () => {
    mockGetStartupPaths.mockResolvedValue(["/cli/music"]);
    const { result } = renderHook(() => useProcessing());
    await waitFor(() => {
      expect(result.current.folders).toContain("/cli/music");
    });
  });

  it("does not set folders if startup paths are empty", async () => {
    mockGetStartupPaths.mockResolvedValue([]);
    const { result } = renderHook(() => useProcessing());
    await waitFor(() => expect(mockGetStartupPaths).toHaveBeenCalled());
    expect(result.current.folders).toEqual([]);
  });
});

describe("useProcessing — addFolder", () => {
  it("adds selected folders to state", async () => {
    mockSelectFolders.mockResolvedValue(["/music/albums"]);
    const { result } = renderHook(() => useProcessing());

    await act(async () => {
      await result.current.addFolder();
    });

    expect(result.current.folders).toContain("/music/albums");
  });

  it("does not add duplicates", async () => {
    mockSelectFolders.mockResolvedValue(["/music"]);
    const { result } = renderHook(() => useProcessing());

    await act(async () => {
      await result.current.addFolder();
      await result.current.addFolder();
    });

    const count = result.current.folders.filter((f) => f === "/music").length;
    expect(count).toBe(1);
  });

  it("does not throw if user cancels (empty selection)", async () => {
    mockSelectFolders.mockResolvedValue([]);
    const { result } = renderHook(() => useProcessing());

    await act(async () => {
      await result.current.addFolder();
    });

    expect(result.current.folders).toEqual([]);
    expect(result.current.error).toBeNull();
  });
});

describe("useProcessing — addFiles", () => {
  it("adds selected files as folder entries", async () => {
    mockSelectFiles.mockResolvedValue(["/music/song.flac"]);
    const { result } = renderHook(() => useProcessing());

    await act(async () => {
      await result.current.addFiles();
    });

    expect(result.current.folders).toContain("/music/song.flac");
  });
});

describe("useProcessing — removeFolder", () => {
  it("removes the specified folder", async () => {
    mockSelectFolders.mockResolvedValue(["/music/a", "/music/b"]);
    const { result } = renderHook(() => useProcessing());

    await act(async () => {
      await result.current.addFolder();
    });

    act(() => {
      result.current.removeFolder("/music/a");
    });

    expect(result.current.folders).not.toContain("/music/a");
    expect(result.current.folders).toContain("/music/b");
  });
});

describe("useProcessing — resetRun", () => {
  it("resets status to idle", async () => {
    const { result } = renderHook(() => useProcessing());

    act(() => {
      result.current.resetRun();
    });

    expect(result.current.status).toBe("idle");
  });

  it("clears workers and events", async () => {
    const { result } = renderHook(() => useProcessing());

    act(() => {
      result.current.resetRun();
    });

    expect(result.current.workers).toEqual([]);
    expect(result.current.recentEvents).toEqual([]);
  });

  it("clears error", async () => {
    const { result } = renderHook(() => useProcessing());

    act(() => {
      result.current.resetRun();
    });

    expect(result.current.error).toBeNull();
  });
});

describe("useProcessing — startRun", () => {
  const settings = { threadCount: 2, logFolder: "/logs", maxRetries: 3 };

  it("does nothing when no folders selected", async () => {
    const { result } = renderHook(() => useProcessing());

    await act(async () => {
      await result.current.startRun(settings);
    });

    expect(mockScanFolders).not.toHaveBeenCalled();
    expect(result.current.status).toBe("idle");
  });

  it("sets error when no FLAC files found", async () => {
    mockSelectFolders.mockResolvedValue(["/empty"]);
    mockScanFolders.mockResolvedValue({ files: [], totalSize: 0 });
    const { result } = renderHook(() => useProcessing());

    // Add folder first, wait for state to settle before calling startRun
    await act(async () => { await result.current.addFolder(); });
    await waitFor(() => expect(result.current.folders).toContain("/empty"));

    await act(async () => { await result.current.startRun(settings); });

    await waitFor(() => expect(result.current.error).toBe("No FLAC files found in the selected folders."));
    expect(result.current.status).toBe("idle");
  });

  it("transitions to processing status when FLAC files found", async () => {
    mockSelectFolders.mockResolvedValue(["/music"]);
    mockScanFolders.mockResolvedValue({ files: ["/music/a.flac"], totalSize: 1000 });
    const { result } = renderHook(() => useProcessing());

    await act(async () => { await result.current.addFolder(); });
    await waitFor(() => expect(result.current.folders).toContain("/music"));

    await act(async () => { await result.current.startRun(settings); });

    await waitFor(() => expect(result.current.status).toBe("processing"));
  });

  it("calls startProcessing with folders and settings", async () => {
    mockSelectFolders.mockResolvedValue(["/music"]);
    mockScanFolders.mockResolvedValue({ files: ["/music/a.flac"], totalSize: 1000 });
    const { result } = renderHook(() => useProcessing());

    await act(async () => { await result.current.addFolder(); });
    await waitFor(() => expect(result.current.folders).toContain("/music"));

    await act(async () => { await result.current.startRun(settings); });

    await waitFor(() => expect(mockStartProcessing).toHaveBeenCalledWith(["/music"], settings));
  });
});

describe("useProcessing — cancelRun", () => {
  it("transitions to cancelling status", async () => {
    const { result } = renderHook(() => useProcessing());

    await act(async () => {
      await result.current.cancelRun();
    });

    expect(result.current.status).toBe("cancelling");
    expect(mockCancelProcessing).toHaveBeenCalled();
  });
});
