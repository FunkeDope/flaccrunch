import { useState, useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import type { WorkerStatus } from "../types/processing";

export function useWorkerStatus(workerCount: number) {
  const [workers, setWorkers] = useState<WorkerStatus[]>(() =>
    Array.from({ length: workerCount }, (_, i) => ({
      id: i,
      state: "idle" as const,
      file: null,
      percent: 0,
      ratio: "",
    }))
  );

  useEffect(() => {
    setWorkers(
      Array.from({ length: workerCount }, (_, i) => ({
        id: i,
        state: "idle" as const,
        file: null,
        percent: 0,
        ratio: "",
      }))
    );
  }, [workerCount]);

  useEffect(() => {
    const unlisten = listen<Record<string, unknown>>("worker-progress", (event) => {
      const { workerId, percent, ratio, file, stage } = event.payload as {
        workerId: number;
        percent: number;
        ratio: string;
        file: string;
        stage: string;
      };
      setWorkers((prev) => {
        const updated = [...prev];
        if (updated[workerId]) {
          updated[workerId] = {
            ...updated[workerId],
            state: (stage as WorkerStatus["state"]) || "idle",
            file,
            percent,
            ratio,
          };
        }
        return updated;
      });
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  return workers;
}
