import { formatBytes } from "../../lib/format";
import type { RunCounters } from "../../types/processing";

interface OverallProgressProps {
  counters: RunCounters;
}

export function OverallProgress({ counters }: OverallProgressProps) {
  const pct =
    counters.totalFiles > 0
      ? (counters.processed / counters.totalFiles) * 100
      : 0;

  return (
    <div className="overall-progress">
      <div style={{ flex: 1 }}>
        <div className="progress-bar">
          <div className="fill" style={{ width: `${pct}%` }} />
        </div>
      </div>
      <div className="counter">
        <strong>{counters.processed}</strong> / {counters.totalFiles} files
      </div>
      <div className="counter">
        Saved: <strong>{formatBytes(counters.totalSavedBytes, true)}</strong>
      </div>
    </div>
  );
}
