import { useState, useEffect, useCallback } from "react";
import { listen } from "@tauri-apps/api/event";
import * as api from "../lib/tauri";
import type {
  RunStatus,
  WorkerStatus,
  RunCounters,
  FileEvent,
  CompressionResult,
  ProcessingSettings,
} from "../types/processing";

const defaultCounters: RunCounters = {
  totalFiles: 0,
  processed: 0,
  successful: 0,
  failed: 0,
  totalOriginalBytes: 0,
  totalNewBytes: 0,
  totalSavedBytes: 0,
  totalMetadataSaved: 0,
  totalPaddingSaved: 0,
  totalArtworkSaved: 0,
  totalArtworkRawSaved: 0,
  artworkOptimizedFiles: 0,
  artworkOptimizedBlocks: 0,
};

export function useProcessing() {
  const [status, setStatus] = useState<RunStatus>("idle");
  const [folders, setFolders] = useState<string[]>([]);
  const [workers, setWorkers] = useState<WorkerStatus[]>([]);
  const [counters, setCounters] = useState<RunCounters>(defaultCounters);
  const [recentEvents, setRecentEvents] = useState<FileEvent[]>([]);
  const [topCompression, setTopCompression] = useState<CompressionResult[]>([]);
  const [startTime, setStartTime] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Listen for pipeline events from the Rust backend
  useEffect(() => {
    const unlisten = listen<Record<string, unknown>>("pipeline-event", (event) => {
      const payload = event.payload;
      const type = payload.type as string;

      switch (type) {
        case "workerStarted":
          setWorkers((prev) => {
            const workerId = payload.workerId as number;
            const updated = [...prev];
            // Grow the array if needed (events can arrive before React processes setWorkers)
            while (updated.length <= workerId) {
              updated.push({ id: updated.length, state: "idle", file: null, percent: 0, ratio: "" });
            }
            updated[workerId] = {
              ...updated[workerId],
              state: "converting",
              file: payload.file as string,
              percent: 0,
              ratio: "",
            };
            return updated;
          });
          break;

        case "workerProgress":
          setWorkers((prev) => {
            const workerId = payload.workerId as number;
            if (workerId >= prev.length) return prev;
            const updated = [...prev];
            updated[workerId] = {
              ...updated[workerId],
              percent: payload.percent as number,
              ratio: (payload.ratio as string) ?? "",
            };
            return updated;
          });
          break;

        case "workerStageChanged": {
          const workerId = payload.workerId as number;
          const stage = payload.stage as string | Record<string, unknown>;
          let stageStr: WorkerStatus["state"] = "idle";
          if (typeof stage === "string") {
            stageStr = stage as WorkerStatus["state"];
          } else if (stage && typeof stage === "object") {
            // Hashing comes as { "hashing": "source" } or { "hashing": "output" }
            if ("hashing" in stage) stageStr = "hashing";
            else if ("converting" in stage) stageStr = "converting";
            else if ("artwork" in stage) stageStr = "artwork";
            else if ("finalizing" in stage) stageStr = "finalizing";
            else if ("complete" in stage) stageStr = "idle";
          }
          setWorkers((prev) => {
            const updated = [...prev];
            while (updated.length <= workerId) {
              updated.push({ id: updated.length, state: "idle", file: null, percent: 0, ratio: "" });
            }
            updated[workerId] = {
              ...updated[workerId],
              state: stageStr,
              percent: 0,
              ratio: "",
            };
            return updated;
          });
          break;
        }

        case "workerIdle":
          setWorkers((prev) => {
            const workerId = payload.workerId as number;
            if (workerId >= prev.length) return prev;
            const updated = [...prev];
            updated[workerId] = {
              ...updated[workerId],
              state: "idle",
              file: null,
              percent: 0,
              ratio: "",
            };
            return updated;
          });
          break;

        case "fileCompleted": {
          const fileEvent = payload.event as FileEvent;
          const eventCounters = payload.counters as RunCounters | undefined;

          // Use backend counter snapshot if available
          if (eventCounters) {
            setCounters(eventCounters);
          }

          if (fileEvent) {
            setRecentEvents((prev) => {
              const updated = [...prev, fileEvent];
              return updated.slice(-25);
            });
            // Update top compression
            if (fileEvent.status === "OK" && fileEvent.savedBytes > 0) {
              setTopCompression((prev) => {
                const updated = [
                  ...prev,
                  {
                    path: fileEvent.file,
                    savedBytes: fileEvent.savedBytes,
                    savedPct: fileEvent.compressionPct,
                  },
                ];
                updated.sort((a, b) => b.savedBytes - a.savedBytes);
                return updated.slice(0, 3);
              });
            }
          }
          break;
        }

        case "runComplete":
          setStatus("complete");
          break;
      }
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const addFolder = useCallback(async () => {
    try {
      const selected = await api.selectFolders();
      if (selected.length > 0) {
        setFolders((prev) => [
          ...prev,
          ...selected.filter((f) => !prev.includes(f)),
        ]);
      }
    } catch {
      // User cancelled or not supported on platform
    }
  }, []);

  const addFiles = useCallback(async () => {
    try {
      const selected = await api.selectFiles();
      if (selected.length > 0) {
        setFolders((prev) => [
          ...prev,
          ...selected.filter((f) => !prev.includes(f)),
        ]);
      }
    } catch {
      // User cancelled
    }
  }, []);

  const removeFolder = useCallback((folder: string) => {
    setFolders((prev) => prev.filter((f) => f !== folder));
  }, []);

  const startRun = useCallback(
    async (settings: ProcessingSettings) => {
      if (folders.length === 0) return;

      setError(null);

      try {
        // Scan first to get file count
        const scan = await api.scanFolders(folders);

        if (scan.files.length === 0) {
          setError("No FLAC files found in the selected folders.");
          return;
        }

        setStatus("processing");
        setStartTime(Date.now());
        setCounters(defaultCounters);
        setRecentEvents([]);
        setTopCompression([]);

        const workerCount = Math.min(
          settings.threadCount,
          scan.files.length
        );
        setWorkers(
          Array.from({ length: workerCount }, (_, i) => ({
            id: i,
            state: "idle" as const,
            file: null,
            percent: 0,
            ratio: "",
          }))
        );
        setCounters((prev) => ({
          ...prev,
          totalFiles: scan.files.length,
          totalOriginalBytes: scan.totalSize,
        }));

        await api.startProcessing(folders, settings);
      } catch (e) {
        setStatus("idle");
        setError(String(e));
      }
    },
    [folders]
  );

  const cancelRun = useCallback(async () => {
    try {
      setStatus("cancelling");
      await api.cancelProcessing();
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const resetRun = useCallback(() => {
    setStatus("idle");
    setCounters(defaultCounters);
    setWorkers([]);
    setRecentEvents([]);
    setTopCompression([]);
    setStartTime(null);
    setError(null);
  }, []);

  return {
    status,
    folders,
    workers,
    counters,
    recentEvents,
    topCompression,
    startTime,
    error,
    addFolder,
    addFiles,
    removeFolder,
    startRun,
    cancelRun,
    resetRun,
  };
}
