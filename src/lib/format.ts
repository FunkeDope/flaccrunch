export function formatBytes(bytes: number, signed = false): string {
  const abs = Math.abs(bytes);
  const sign = bytes < 0 ? "-" : signed ? "+" : "";

  if (abs < 1024) return `${sign}${abs} B`;
  if (abs < 1024 * 1024) return `${sign}${(abs / 1024).toFixed(2)} KB`;
  if (abs < 1024 * 1024 * 1024)
    return `${sign}${(abs / (1024 * 1024)).toFixed(2)} MB`;
  return `${sign}${(abs / (1024 * 1024 * 1024)).toFixed(2)} GB`;
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
    case "converting": return "stage-converting";
    case "hashing": return "stage-hashing";
    case "artwork": return "stage-artwork";
    case "finalizing": return "stage-finalizing";
    default: return "stage-idle";
  }
}

export function getStatusColor(status: string): string {
  switch (status) {
    case "OK": return "status-ok";
    case "FAIL": return "status-fail";
    case "RETRY": return "status-retry";
    default: return "";
  }
}
