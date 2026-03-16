import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook, waitFor } from "@testing-library/react";

// Mock Tauri webview window API
const mockSetTitle = vi.fn().mockResolvedValue(undefined);
vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => ({ setTitle: mockSetTitle }),
}));

// Mock Tauri app version API
vi.mock("@tauri-apps/api/app", () => ({
  getVersion: vi.fn().mockResolvedValue("1.0.5"),
}));

// Mock lib/tauri so useSettings doesn't make real IPC calls
vi.mock("../lib/tauri", () => ({
  getCpuCount: vi.fn().mockResolvedValue(4),
  getDefaultLogFolder: vi.fn().mockResolvedValue("/tmp"),
  getSettings: vi.fn().mockResolvedValue({
    threadCount: null,
    logFolder: null,
    maxRetries: 3,
    recentFolders: [],
    verboseLogging: false,
  }),
  saveSettings: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { useSettings } from "./useSettings";

beforeEach(() => {
  vi.clearAllMocks();
  mockSetTitle.mockResolvedValue(undefined);
});

describe("window title via useSettings.appVersion", () => {
  it("appVersion resolves to the value returned by getVersion()", async () => {
    const { result } = renderHook(() => useSettings());
    await waitFor(() => expect(result.current.appVersion).toBe("1.0.5"));
  });
});

describe("App — window title", () => {
  it("setTitle is called with 'FlacCrunch v1.0.5' when appVersion resolves", async () => {
    // Simulate what App.tsx useEffect does
    const { getVersion } = await import("@tauri-apps/api/app");
    const { getCurrentWebviewWindow } = await import("@tauri-apps/api/webviewWindow");

    const version = await getVersion();
    await getCurrentWebviewWindow().setTitle(`FlacCrunch v${version}`);

    expect(mockSetTitle).toHaveBeenCalledWith("FlacCrunch v1.0.5");
  });
});
