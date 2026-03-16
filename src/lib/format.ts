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
    default: return "";
  }
}

/** Returns a CSS class for color-coding a row by compression ratio. */
export function compressionRowClass(pct: number): string {
  if (pct >= 20) return "comp-excellent";
  if (pct >= 10) return "comp-good";
  if (pct >= 5) return "comp-fair";
  if (pct > 0) return "comp-poor";
  return "comp-none";
}

/** Download text content as a file in the browser / Tauri webview. */
export function downloadTextFile(content: string, filename: string): void {
  const blob = new Blob([content], { type: "text/plain;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}
