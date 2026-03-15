import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook, act } from "@testing-library/react";

// Mock Tauri event API
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(),
}));

import { listen } from "@tauri-apps/api/event";
import { useWorkerStatus } from "./useWorkerStatus";

const mockListen = vi.mocked(listen);

beforeEach(() => {
  vi.clearAllMocks();
  // Default: listen returns a cleanup function
  mockListen.mockResolvedValue(vi.fn());
});

describe("useWorkerStatus — initial state", () => {
  it("initializes workers array with correct length", () => {
    const { result } = renderHook(() => useWorkerStatus(3));
    expect(result.current).toHaveLength(3);
  });

  it("initializes workers with idle state", () => {
    const { result } = renderHook(() => useWorkerStatus(2));
    result.current.forEach((w) => {
      expect(w.state).toBe("idle");
      expect(w.file).toBeNull();
      expect(w.percent).toBe(0);
      expect(w.ratio).toBe("");
    });
  });

  it("assigns correct ids (0-indexed)", () => {
    const { result } = renderHook(() => useWorkerStatus(3));
    expect(result.current[0].id).toBe(0);
    expect(result.current[1].id).toBe(1);
    expect(result.current[2].id).toBe(2);
  });

  it("resets workers when workerCount changes", () => {
    const { result, rerender } = renderHook(({ count }) => useWorkerStatus(count), {
      initialProps: { count: 2 },
    });
    expect(result.current).toHaveLength(2);

    rerender({ count: 4 });
    expect(result.current).toHaveLength(4);
    result.current.forEach((w) => expect(w.state).toBe("idle"));
  });
});

describe("useWorkerStatus — event listener registration", () => {
  it("calls listen to register worker-progress handler", () => {
    renderHook(() => useWorkerStatus(2));
    expect(mockListen).toHaveBeenCalledWith("worker-progress", expect.any(Function));
  });

  it("cleans up listener on unmount", async () => {
    const mockUnlisten = vi.fn();
    mockListen.mockResolvedValue(mockUnlisten);

    const { unmount } = renderHook(() => useWorkerStatus(2));
    unmount();

    // Give promise time to resolve
    await act(async () => {});
    expect(mockUnlisten).toHaveBeenCalled();
  });
});

describe("useWorkerStatus — event handling", () => {
  it("updates worker state when worker-progress event fires", async () => {
    let capturedHandler: ((event: { payload: unknown }) => void) | null = null;
    mockListen.mockImplementation((_channel, handler) => {
      capturedHandler = handler as typeof capturedHandler;
      return Promise.resolve(vi.fn());
    });

    const { result } = renderHook(() => useWorkerStatus(2));

    await act(async () => {
      capturedHandler?.({
        payload: {
          workerId: 0,
          percent: 55,
          ratio: "0.42",
          file: "/music/track.flac",
          stage: "converting",
        },
      });
    });

    expect(result.current[0].state).toBe("converting");
    expect(result.current[0].percent).toBe(55);
    expect(result.current[0].ratio).toBe("0.42");
    expect(result.current[0].file).toBe("/music/track.flac");
  });

  it("does not update workers beyond valid index", async () => {
    let capturedHandler: ((event: { payload: unknown }) => void) | null = null;
    mockListen.mockImplementation((_channel, handler) => {
      capturedHandler = handler as typeof capturedHandler;
      return Promise.resolve(vi.fn());
    });

    const { result } = renderHook(() => useWorkerStatus(2));

    await act(async () => {
      // workerId=5 is out of bounds for 2 workers — should not crash
      capturedHandler?.({
        payload: {
          workerId: 5,
          percent: 10,
          ratio: "",
          file: "/bad.flac",
          stage: "converting",
        },
      });
    });

    // Workers still have length 2 and are idle
    expect(result.current).toHaveLength(2);
    expect(result.current[0].state).toBe("idle");
    expect(result.current[1].state).toBe("idle");
  });
});
