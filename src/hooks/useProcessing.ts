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

  // Listen for pipeline events from the Rust backend
  useEffect(() => {
    const unlisten = listen<Record<string, unknown>>("pipeline-event", (event) => {
      const payload = event.payload;
      const type = payload.type as string;

      switch (type) {
        case "workerStarted":
        case "workerProgress":
        case "workerStageChanged":
        case "workerIdle":
          // Update worker status
          setWorkers((prev) => {
            const workerId = payload.workerId as number ?? payload.worker_id as number;
            const updated = [...prev];
            if (updated[workerId]) {
              if (type === "workerStarted") {
                updated[workerId] = {
                  ...updated[workerId],
                  state: "converting",
                  file: payload.file as string,
                  percent: 0,
                  ratio: "",
                };
              } else if (type === "workerProgress") {
                updated[workerId] = {
                  ...updated[workerId],
                  percent: payload.percent as number,
                  ratio: payload.ratio as string,
                };
              } else if (type === "workerIdle") {
                updated[workerId] = {
                  ...updated[workerId],
                  state: "idle",
                  file: null,
                  percent: 0,
                  ratio: "",
                };
              }
            }
            return updated;
          });
          break;

        case "fileCompleted": {
          const fileEvent = payload.event as FileEvent;
          if (fileEvent) {
            setRecentEvents((prev) => {
              const updated = [...prev, fileEvent];
              return updated.slice(-25);
            });
            setCounters((prev) => ({
              ...prev,
              processed: prev.processed + 1,
              successful:
                fileEvent.status === "OK"
                  ? prev.successful + 1
                  : prev.successful,
              failed:
                fileEvent.status === "FAIL"
                  ? prev.failed + 1
                  : prev.failed,
              totalSavedBytes: prev.totalSavedBytes + fileEvent.savedBytes,
            }));
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
      // User cancelled
    }
  }, []);

  const removeFolder = useCallback((folder: string) => {
    setFolders((prev) => prev.filter((f) => f !== folder));
  }, []);

  const startRun = useCallback(
    async (settings: ProcessingSettings) => {
      if (folders.length === 0) return;
      try {
        setStatus("processing");
        setStartTime(Date.now());
        setCounters(defaultCounters);
        setRecentEvents([]);
        setTopCompression([]);

        // Scan first to get file count for worker initialization
        const scan = await api.scanFolders(folders);
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
        console.error("Failed to start processing:", e);
      }
    },
    [folders]
  );

  const cancelRun = useCallback(async () => {
    try {
      setStatus("cancelling");
      await api.cancelProcessing();
    } catch (e) {
      console.error("Failed to cancel:", e);
    }
  }, []);

  return {
    status,
    folders,
    workers,
    counters,
    recentEvents,
    topCompression,
    startTime,
    addFolder,
    removeFolder,
    startRun,
    cancelRun,
  };
}
