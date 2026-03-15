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
  sourceHash?: string;
  outputHash?: string;
  embeddedMd5?: string;
  artworkSavedBytes: number;
  artworkRawSavedBytes: number;
  artworkBlocksOptimized: number;
}

export interface CompressionResult {
  path: string;
  savedBytes: number;
  savedPct: number;
  beforeSize: number;
  afterSize: number;
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

export interface JobRecord {
  id: string;
  startTime: number;
  endTime: number;
  folders: string[];
  counters: RunCounters;
  events: FileEvent[];
  topCompression: CompressionResult[];
}
