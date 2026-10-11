export type RunStatus = "idle" | "scanning" | "processing" | "cancelling" | "complete";

export interface WorkerStatus {
  id: number;
  state: "idle" | "converting" | "hashing-source" | "hashing-output" | "artwork" | "finalizing";
  file: string | null;
  percent: number;
  ratio: string;
  // Populated after the most recent file completes on this worker
  lastSourceHash?: string;
  lastOutputHash?: string;
  lastEmbeddedMd5?: string;
  lastVerification?: string;
  lastCompressionPct?: number;
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
  warned: number;
  skipped: number;
}

export interface FileEvent {
  time: string;
  status: "OK" | "WARN" | "RETRY" | "FAIL" | "SKIPPED";
  file: string;
  attempt: string;
  verification: string;
  beforeSize: number;
  afterSize: number;
  savedBytes: number;
  compressionPct: number;
  detail: string;
  sourceHash?: string;
  outputHash?: string;
  embeddedMd5?: string;
  /** Net bytes saved inside PICTURE blocks. */
  artworkSavedBytes: number;
  artworkRawSavedBytes: number;
  artworkBlocksOptimized: number;
  /** Bytes of PADDING removed. Optional for events produced by older backends. */
  paddingSavedBytes?: number;
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
  verboseLogging: boolean;
  markAsCrunched: boolean;
  skipCrunched: boolean;
  /** Non-subset maximum compression (smaller; some old hardware players reject it). */
  maxCompression: boolean;
}
