import { formatBytes } from "../../lib/format";
import type { RunCounters } from "../../types/processing";

interface StatsBarProps {
  counters: RunCounters;
}

export function StatsBar({ counters }: StatsBarProps) {
  return (
    <div className="stats-bar">
      <div className="stat-item">
        <div className="stat-value stat-success">{counters.successful}</div>
        <div className="stat-label">Succeeded</div>
      </div>
      <div className="stat-item">
        <div className={`stat-value ${counters.failed > 0 ? "stat-error" : ""}`}>
          {counters.failed}
        </div>
        <div className="stat-label">Failed</div>
      </div>
      <div className="stat-item">
        <div className="stat-value">
          {formatBytes(counters.totalSavedBytes)}
        </div>
        <div className="stat-label">Total Saved</div>
      </div>
      <div className="stat-item">
        <div className="stat-value">
          {formatBytes(counters.totalArtworkSaved)}
        </div>
        <div className="stat-label">Artwork Saved</div>
      </div>
    </div>
  );
}
