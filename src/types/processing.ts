export type RunStatus = "idle" | "scanning" | "processing" | "cancelling" | "complete";

export interface WorkerStatus {
  id: number;
  state: "idle" | "converting" | "hashing" | "artwork" | "finalizing";
  file: string | null;
  percent: number;
  ratio: string;
}

export interface RunCounters {
  totalFiles: number;
  processed: number;
  successful: number;
  failed: number;
  totalOriginalBytes: number;
  totalNewBytes: number;
  totalSavedBytes: number;
  totalMetadataSaved: number;
  totalPaddingSaved: number;
  totalArtworkSaved: number;
  totalArtworkRawSaved: number;
  artworkOptimizedFiles: number;
  artworkOptimizedBlocks: number;
}

export interface FileEvent {
  time: string;
  status: "OK" | "RETRY" | "FAIL";
  file: string;
  attempt: string;
  verification: string;
  beforeSize: number;
  afterSize: number;
  savedBytes: number;
  compressionPct: number;
  detail: string;
}

export interface CompressionResult {
  path: string;
  savedBytes: number;
  savedPct: number;
}

export interface ScanResult {
  files: { path: string; name: string; size: number }[];
  permissionErrors: string[];
  totalSize: number;
}

export interface ProcessingSettings {
  threadCount: number;
  logFolder: string;
  maxRetries: number;
}
