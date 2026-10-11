import { useState, useEffect, useCallback, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { save as tauriSaveDialog } from "@tauri-apps/plugin-dialog";
import * as api from "../lib/tauri";
import type {
  RunStatus,
  WorkerStatus,
  RunCounters,
  FileEvent,
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
  warned: 0,
  skipped: 0,
};

function idleWorker(id: number): WorkerStatus {
  return { id, state: "idle", file: null, percent: 0, ratio: "" };
}

/** Map a backend `PipelineStage` payload to the worker state string. */
function stageToState(stage: unknown): WorkerStatus["state"] {
  if (typeof stage === "string") {
    return stage === "complete" ? "idle" : (stage as WorkerStatus["state"]);
  }
  if (stage && typeof stage === "object" && "hashing" in stage) {
    return (stage as { hashing: string }).hashing === "output" ? "hashing-output" : "hashing-source";
  }
  return "idle";
}

/** Timestamp suitable for a default log filename. */
function logTimestamp(ms: number): string {
  return new Date(ms).toISOString().slice(0, 19).replace(/[T:]/g, "-");
}

async function saveLogWithDialog(title: string, filename: string, content: string) {
  try {
    const path = await tauriSaveDialog({
      title,
      defaultPath: filename,
      filters: [{ name: "Text files", extensions: ["txt"] }],
    });
    if (path) await api.writeTextFile(path, content);
  } catch {
    // Dialog cancelled or write failed — nothing to surface here.
  }
}

export function useProcessing() {
  const [status, setStatus] = useState<RunStatus>("idle");
  const [folders, setFolders] = useState<string[]>([]);
  const [workers, setWorkers] = useState<WorkerStatus[]>([]);
  const [counters, setCounters] = useState<RunCounters>(defaultCounters);
  const [allEvents, setAllEvents] = useState<FileEvent[]>([]);
  const [startTime, setStartTime] = useState<number | null>(null);
  const [endTime, setEndTime] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isDragOver, setIsDragOver] = useState(false);
  // Sources of the run in progress (grows with live drops).
  const [currentFolders, setCurrentFolders] = useState<string[]>([]);
  // Set by the drop handler when files land while status is "complete".
  // App.tsx watches this and auto-starts a fresh run with the dropped paths.
  const [autoStartPaths, setAutoStartPaths] = useState<string[] | null>(null);

  // Mirrors of state that async callbacks need to read at call time without
  // re-subscribing (listeners are registered once on mount). Reading state
  // through nested setState updaters — the previous approach — is impure and
  // double-fires under StrictMode.
  const statusRef = useRef<RunStatus>("idle");
  const eventsRef = useRef<FileEvent[]>([]);
  const countersRef = useRef<RunCounters>(defaultCounters);
  const startTimeRef = useRef<number | null>(null);
  const endTimeRef = useRef<number | null>(null);
  const runSettingsRef = useRef<ProcessingSettings | null>(null);
  const currentFoldersRef = useRef<string[]>([]);
  const cancelledRef = useRef(false);

  useEffect(() => { statusRef.current = status; }, [status]);
  useEffect(() => { eventsRef.current = allEvents; }, [allEvents]);
  useEffect(() => { countersRef.current = counters; }, [counters]);
  useEffect(() => { startTimeRef.current = startTime; }, [startTime]);
  useEffect(() => { endTimeRef.current = endTime; }, [endTime]);
  useEffect(() => { currentFoldersRef.current = currentFolders; }, [currentFolders]);

  // On mount: load any paths supplied via CLI args
  useEffect(() => {
    api.getStartupPaths().then((paths) => {
      if (paths.length > 0) {
        setFolders(paths);
      }
    }).catch(() => {});
  }, []);

  useEffect(() => {
    const unlisten = listen<Record<string, unknown>>("pipeline-event", (event) => {
      const payload = event.payload;
      const type = payload.type as string;
      const workerId = payload.workerId as number;

      switch (type) {
        case "workerStarted":
          setWorkers((prev) => {
            const updated = [...prev];
            while (updated.length <= workerId) updated.push(idleWorker(updated.length));
            updated[workerId] = {
              ...updated[workerId],
              state: stageToState(payload.stage),
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

        case "workerProgress":
          setWorkers((prev) => {
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

        case "workerStageChanged":
          setWorkers((prev) => {
            const updated = [...prev];
            while (updated.length <= workerId) updated.push(idleWorker(updated.length));
            updated[workerId] = { ...updated[workerId], state: stageToState(payload.stage), percent: 0, ratio: "" };
            return updated;
          });
          break;

        case "workerHashComputed": {
          const phase = payload.phase as string;
          const hash = payload.hash as string;
          const embeddedMd5 = payload.embeddedMd5 as string | null | undefined;
          setWorkers((prev) => {
            if (workerId >= prev.length) return prev;
            const updated = [...prev];
            if (phase === "source") {
              updated[workerId] = { ...updated[workerId], lastSourceHash: hash, lastEmbeddedMd5: embeddedMd5 ?? undefined };
            } else if (phase === "output") {
              updated[workerId] = { ...updated[workerId], lastOutputHash: hash };
            }
            return updated;
          });
          break;
        }

        case "workerIdle":
          setWorkers((prev) => {
            if (workerId >= prev.length) return prev;
            const updated = [...prev];
            // Keep file so the card still shows the filename next to the hashes.
            // It will be overwritten when the next workerStarted fires.
            updated[workerId] = { ...updated[workerId], state: "idle", percent: 0, ratio: "" };
            return updated;
          });
          break;

        case "fileCompleted": {
          const fileEvent = payload.event as FileEvent | undefined;
          const eventCounters = payload.counters as RunCounters | undefined;
          if (eventCounters) setCounters(eventCounters);
          if (!fileEvent) break;

          setAllEvents((prev) => [...prev, fileEvent]);

          // Mark worker idle immediately so livePct doesn't double-count
          // (processed counter just incremented; worker contribution must drop to 0).
          const succeeded = fileEvent.status === "OK" || fileEvent.status === "WARN";
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
              lastCompressionPct: succeeded ? fileEvent.compressionPct : undefined,
            };
            return updated;
          });
          break;
        }

        case "queueGrew": {
          const newCounters = payload.counters as RunCounters | undefined;
          if (newCounters) setCounters(newCounters);
          break;
        }

        case "runComplete":
          setEndTime(Date.now());
          setStatus("complete");
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
        const dropped = payload.paths;
        const currentStatus = statusRef.current;
        if (currentStatus === "processing" || currentStatus === "cancelling") {
          // Mid-run: append to the live Rust queue instead of the FE list.
          api.enqueueFiles(dropped).catch(() => {});
          // Reflect the source paths in the run's folder list so the
          // "Show queue" panel can display what's been added mid-run.
          setCurrentFolders((prev) => [...prev, ...dropped.filter((p) => !prev.includes(p))]);
        } else if (currentStatus === "complete") {
          // Run is over and the user dropped more work — auto-start a new run.
          setAutoStartPaths(dropped);
        } else {
          setFolders((prev) => [...prev, ...dropped.filter((p) => !prev.includes(p))]);
        }
      }
    });
    return () => {
      unlistenPromise.then((fn) => fn());
    };
  }, []);

  /** Build the EFC log for the current/last run from the latest state. */
  const getLogText = useCallback((): Promise<string> => {
    const now = endTimeRef.current ?? Date.now();
    const startedAt = startTimeRef.current ?? now;
    const rs = runSettingsRef.current;
    return api.getEfcLog(
      eventsRef.current,
      Math.round((now - startedAt) / 1000),
      currentFoldersRef.current.join(", "),
      startedAt,
      now,
      rs?.threadCount ?? 1,
      rs?.maxRetries ?? 3,
      cancelledRef.current,
    );
  }, []);

  // When a run completes with failures, offer to save the error log.
  // A run the user cancelled is not an error condition.
  useEffect(() => {
    if (status !== "complete") return;
    const c = countersRef.current;
    if (c.failed === 0 || cancelledRef.current) return;
    const finishedAt = endTimeRef.current ?? Date.now();
    getLogText()
      .then((log) => saveLogWithDialog("Save Error Log", `flaccrunch-errors-${logTimestamp(finishedAt)}.txt`, log))
      .catch(() => {});
  }, [status, getLogText]);

  const exportLog = useCallback(() => {
    getLogText()
      .then((log) => saveLogWithDialog("Save Log", `flaccrunch-log-${logTimestamp(Date.now())}.txt`, log))
      .catch(() => {});
  }, [getLogText]);

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
    async (settings: ProcessingSettings, pathsOverride?: string[]) => {
      const targets = pathsOverride ?? folders;
      if (targets.length === 0) return;

      setError(null);

      try {
        const scan = await api.scanFolders(targets);
        if (scan.files.length === 0) {
          setError("No FLAC files found in the selected folders.");
          return;
        }

        if (pathsOverride) setFolders(pathsOverride);
        setCurrentFolders(targets);
        currentFoldersRef.current = targets;
        setStatus("processing");
        const now = Date.now();
        setStartTime(now);
        setEndTime(null);
        startTimeRef.current = now;
        endTimeRef.current = null;
        runSettingsRef.current = settings;
        cancelledRef.current = false;
        setAllEvents([]);
        eventsRef.current = [];

        const workerCount = Math.min(settings.threadCount, scan.files.length);
        setWorkers(Array.from({ length: workerCount }, (_, i) => idleWorker(i)));
        const initial = { ...defaultCounters, totalFiles: scan.files.length, totalOriginalBytes: scan.totalSize };
        setCounters(initial);
        countersRef.current = initial;

        await api.startProcessing(targets, settings);
      } catch (e) {
        setStatus("idle");
        setError(String(e));
      }
    },
    [folders]
  );

  const cancelRun = useCallback(async () => {
    try {
      cancelledRef.current = true;
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
    setStartTime(null);
    setEndTime(null);
    setError(null);
    setFolders([]);
    setCurrentFolders([]);
    cancelledRef.current = false;
  }, []);

  const testStorage = useCallback(async () => {
    try {
      setError(null);
      const selected = await api.selectFiles();
      const source = selected[0];
      if (!source) return;

      const sourceName = source.split("/").pop() || "storage-test.bin";
      const testName = sourceName.includes(".")
        ? sourceName.replace(/(\.[^.]+)$/u, "-storage-test$1")
        : `${sourceName}-storage-test`;

      const destination = await tauriSaveDialog({
        title: "Storage Test — Save Copy",
        defaultPath: testName,
      });
      if (!destination) return;

      await api.copyFileToPath(source, destination);
    } catch (e) {
      setError(`Storage test failed: ${String(e)}`);
    }
  }, []);

  const clearAutoStartPaths = useCallback(() => setAutoStartPaths(null), []);

  return {
    status,
    folders,
    runFolders: currentFolders,
    workers,
    counters,
    recentEvents: allEvents,
    startTime,
    endTime,
    error,
    isDragOver,
    autoStartPaths,
    clearAutoStartPaths,
    addFolder,
    addFiles,
    removeFolder,
    startRun,
    cancelRun,
    resetRun,
    exportLog,
    getLogText,
    testStorage,
  };
}
