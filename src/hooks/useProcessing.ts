import { useState, useEffect, useCallback } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { save as tauriSaveDialog } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import * as api from "../lib/tauri";
import type {
  RunStatus,
  WorkerStatus,
  RunCounters,
  FileEvent,
  JobRecord,
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
  const [allEvents, setAllEvents] = useState<FileEvent[]>([]);
  const [topCompression, setTopCompression] = useState<FileEvent[]>([]);
  const [startTime, setStartTime] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [jobHistory, setJobHistory] = useState<JobRecord[]>([]);

  const [isDragOver, setIsDragOver] = useState(false);
  const [currentFolders, setCurrentFolders] = useState<string[]>([]);

  // On mount: load any paths supplied via CLI args
  useEffect(() => {
    api.getStartupPaths().then((paths) => {
      if (paths.length > 0) {
        setFolders(paths);
      }
    }).catch(() => {});
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const unlisten = listen<Record<string, unknown>>("pipeline-event", (event) => {
      const payload = event.payload;
      const type = payload.type as string;

      switch (type) {
        case "workerStarted": {
          const workerId = payload.workerId as number;
          const stage = payload.stage as string | Record<string, unknown>;
          let stageStr: WorkerStatus["state"] = "converting";
          if (stage && typeof stage === "object" && "hashing" in stage) {
            stageStr = "hashing-source";
          }
          setWorkers((prev) => {
            const updated = [...prev];
            while (updated.length <= workerId) {
              updated.push({ id: updated.length, state: "idle", file: null, percent: 0, ratio: "" });
            }
            updated[workerId] = {
              ...updated[workerId],
              state: stageStr,
              file: payload.file as string,
              percent: 0,
              ratio: "",
              // Clear previous file's hashes when a new file starts
              lastSourceHash: undefined,
              lastOutputHash: undefined,
              lastEmbeddedMd5: undefined,
              lastVerification: undefined,
            };
            return updated;
          });
          break;
        }

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
            if ("hashing" in stage) {
              const phase = (stage as Record<string, unknown>).hashing;
              stageStr = phase === "output" ? "hashing-output" : "hashing-source";
            } else if ("converting" in stage) stageStr = "converting";
            else if ("artwork" in stage) stageStr = "artwork";
            else if ("finalizing" in stage) stageStr = "finalizing";
            else if ("complete" in stage) stageStr = "idle";
          }
          setWorkers((prev) => {
            const updated = [...prev];
            while (updated.length <= workerId) {
              updated.push({ id: updated.length, state: "idle", file: null, percent: 0, ratio: "" });
            }
            updated[workerId] = { ...updated[workerId], state: stageStr, percent: 0, ratio: "" };
            return updated;
          });
          break;
        }

        case "workerHashComputed": {
          const workerId = payload.workerId as number;
          const phase = payload.phase as string;
          const hash = payload.hash as string;
          const embeddedMd5 = payload.embeddedMd5 as string | null | undefined;
          setWorkers((prev) => {
            if (workerId >= prev.length) return prev;
            const updated = [...prev];
            if (phase === "source") {
              updated[workerId] = {
                ...updated[workerId],
                lastSourceHash: hash,
                lastEmbeddedMd5: embeddedMd5 ?? undefined,
              };
            } else if (phase === "output") {
              updated[workerId] = {
                ...updated[workerId],
                lastOutputHash: hash,
              };
            }
            return updated;
          });
          break;
        }

        case "workerIdle":
          setWorkers((prev) => {
            const workerId = payload.workerId as number;
            if (workerId >= prev.length) return prev;
            const updated = [...prev];
            // Keep file so the card still shows the filename next to the hashes.
            // It will be overwritten when the next workerStarted fires.
            updated[workerId] = { ...updated[workerId], state: "idle", percent: 0, ratio: "" };
            return updated;
          });
          break;

        case "fileCompleted": {
          const fileEvent = payload.event as FileEvent;
          const eventCounters = payload.counters as RunCounters | undefined;
          const workerId = payload.workerId as number;

          if (eventCounters) {
            setCounters(eventCounters);
          }

          if (fileEvent) {
            setAllEvents((prev) => [...prev, fileEvent]);

            // Mark worker idle immediately so livePct doesn't double-count
            // (processed counter just incremented; worker contribution must drop to 0).
            setWorkers((prev) => {
              if (workerId >= prev.length) return prev;
              const updated = [...prev];
              updated[workerId] = {
                ...updated[workerId],
                state: "idle",
                percent: 0,
                ratio: "",
                lastSourceHash: fileEvent.sourceHash,
                lastOutputHash: fileEvent.outputHash,
                lastEmbeddedMd5: fileEvent.embeddedMd5,
                lastVerification: fileEvent.verification,
                lastCompressionPct: fileEvent.status === "OK" ? fileEvent.compressionPct : undefined,
              };
              return updated;
            });

            if (fileEvent.status === "OK" && fileEvent.savedBytes > 0) {
              setTopCompression((prev) => {
                const updated = [...prev, fileEvent];
                updated.sort((a, b) => b.savedBytes - a.savedBytes);
                return updated.slice(0, 3);
              });
            }
          }
          break;
        }

        case "runComplete":
          setStatus("complete");
          // Snapshot will be handled via a useEffect watching status + allEvents
          break;
      }
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  // Native drag-and-drop from the OS file manager (Windows Explorer, Finder, etc.)
  useEffect(() => {
    const appWindow = getCurrentWebviewWindow();
    const unlistenPromise = appWindow.onDragDropEvent((event) => {
      const payload = event.payload as { type: string; paths?: string[] };
      if (payload.type === "enter" || payload.type === "over") {
        setIsDragOver(true);
      } else if (payload.type === "leave") {
        setIsDragOver(false);
      } else if (payload.type === "drop" && payload.paths && payload.paths.length > 0) {
        setIsDragOver(false);
        setFolders((prev) => [
          ...prev,
          ...payload.paths!.filter((p) => !prev.includes(p)),
        ]);
      }
    });
    return () => {
      unlistenPromise.then((fn) => fn());
    };
  }, []);

  // When run completes: save to job history and auto-export error log if needed
  useEffect(() => {
    if (status !== "complete") return;

    setAllEvents((events) => {
      setCounters((c) => {
        setStartTime((st) => {
          const endTime = Date.now();

          // Save job record
          const job: JobRecord = {
            id: `job-${st ?? endTime}`,
            startTime: st ?? endTime,
            endTime,
            folders: currentFolders,
            counters: c,
            events,
            topCompression: [],
          };
          setJobHistory((prev) => [...prev, job]);

          // Auto-prompt to save error log if there are failures
          if (c.failed > 0) {
            const elapsedSecs = Math.round((endTime - (st ?? endTime)) / 1000);
            const ts = new Date(endTime).toISOString().slice(0, 19).replace(/[T:]/g, "-");
            const filename = `flaccrunch-errors-${ts}.txt`;
            api.getEfcLog(events, elapsedSecs).then((logContent) => {
              tauriSaveDialog({
                title: "Save Error Log",
                defaultPath: filename,
                filters: [{ name: "Text files", extensions: ["txt"] }],
              }).then((path) => {
                if (path) invoke("write_text_file", { path, content: logContent }).catch(() => {});
              }).catch(() => {});
            }).catch(() => {});
          }

          return st;
        });
        return c;
      });
      return events;
    });
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [status]);

  const exportLog = useCallback(() => {
    setAllEvents((events) => {
      setStartTime((st) => {
        const elapsedSecs = Math.round((Date.now() - (st ?? Date.now())) / 1000);
        const ts = new Date().toISOString().slice(0, 19).replace(/[T:]/g, "-");
        const filename = `flaccrunch-log-${ts}.txt`;
        api.getEfcLog(events, elapsedSecs).then((logContent) => {
          tauriSaveDialog({
            title: "Save Log",
            defaultPath: filename,
            filters: [{ name: "Text files", extensions: ["txt"] }],
          }).then((path) => {
            if (path) invoke("write_text_file", { path, content: logContent }).catch(() => {});
          }).catch(() => {});
        }).catch(() => {});
        return st;
      });
      return events;
    });
  }, []);

  const addFolder = useCallback(async () => {
    try {
      const selected = await api.selectFolders();
      if (selected.length > 0) {
        setFolders((prev) => [...prev, ...selected.filter((f) => !prev.includes(f))]);
      }
    } catch {
      // User cancelled
    }
  }, []);

  const addFiles = useCallback(async () => {
    try {
      const selected = await api.selectFiles();
      if (selected.length > 0) {
        setFolders((prev) => [...prev, ...selected.filter((f) => !prev.includes(f))]);
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
        const scan = await api.scanFolders(folders);

        if (scan.files.length === 0) {
          setError("No FLAC files found in the selected folders.");
          return;
        }

        setCurrentFolders(folders);
        setStatus("processing");
        const now = Date.now();
        setStartTime(now);
        setCounters(defaultCounters);
        setAllEvents([]);
        setTopCompression([]);

        const workerCount = Math.min(settings.threadCount, scan.files.length);
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
    setAllEvents([]);
    setTopCompression([]);
    setStartTime(null);
    setError(null);
  }, []);

  return {
    status,
    folders,
    workers,
    counters,
    recentEvents: allEvents,
    topCompression,
    startTime,
    error,
    jobHistory,
    isDragOver,
    addFolder,
    addFiles,
    removeFolder,
    startRun,
    cancelRun,
    resetRun,
    exportLog,
  };
}
