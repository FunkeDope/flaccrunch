export function formatBytes(bytes: number): string {
  if (bytes <= 0) return "0 B";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(2)} KB`;
  if (bytes < 1024 * 1024 * 1024)
    return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

export function formatElapsed(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = Math.floor(seconds % 60);

  if (h >= 24) {
    const d = Math.floor(h / 24);
    const rh = h % 24;
    return `${d}d ${String(rh).padStart(2, "0")}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  }
  return `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
}

export function formatPercent(value: number): string {
  return `${value.toFixed(2)}%`;
}

export function getStageColor(stage: string): string {
  switch (stage) {
    case "converting":      return "stage-converting";
    case "hashing-source":  return "stage-hashing-source";
    case "hashing-output":  return "stage-hashing-output";
    case "artwork":         return "stage-artwork";
    case "finalizing":      return "stage-finalizing";
    default:                return "stage-idle";
  }
}

export function getStageLabel(stage: string): string {
  switch (stage) {
    case "converting":      return "Converting";
    case "hashing-source":  return "Hash src";
    case "hashing-output":  return "Hash out";
    case "artwork":         return "Art opt";
    case "finalizing":      return "Finalizing";
    default:                return "Idle";
  }
}

export function getStatusColor(status: string): string {
  switch (status) {
    case "OK": return "status-ok";
    case "WARN": return "status-warn";
    case "FAIL": return "status-fail";
    case "RETRY": return "status-retry";
    case "SKIPPED": return "status-skipped";
    default: return "";
  }
}

/** Parent folder of a path (handles both separators); the path itself if it has none. */
export function parentFolder(path: string): string {
  const idx = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return idx > 0 ? path.slice(0, idx) : path;
}

/**
 * Folders containing files that failed, deduplicated, with failure counts,
 * sorted by folder name. Each folder is usually one album, so this is the
 * list of albums that need a replacement rip.
 */
export function failedFolders(events: { status: string; file: string }[]): { folder: string; count: number }[] {
  const counts = new Map<string, number>();
  for (const e of events) {
    if (e.status !== "FAIL") continue;
    const folder = parentFolder(e.file);
    counts.set(folder, (counts.get(folder) ?? 0) + 1);
  }
  return [...counts.entries()]
    .map(([folder, count]) => ({ folder, count }))
    .sort((a, b) => a.folder.localeCompare(b.folder));
}

/** Returns a CSS class for color-coding a row by compression ratio. */
export function compressionRowClass(pct: number): string {
  if (pct >= 20) return "comp-excellent";
  if (pct >= 10) return "comp-good";
  if (pct >= 5) return "comp-fair";
  if (pct > 0) return "comp-poor";
  return "comp-none";
}
