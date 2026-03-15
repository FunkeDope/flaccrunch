import { Header } from "../layout/Header";
import { formatBytes, formatPercent } from "../../lib/format";
import type { RunCounters } from "../../types/processing";

interface SummaryViewProps {
  counters: RunCounters;
}

export function SummaryView({ counters }: SummaryViewProps) {
  const successRate =
    counters.totalFiles > 0
      ? (counters.successful / counters.totalFiles) * 100
      : 0;

  const avgSaved =
    counters.successful > 0
      ? counters.totalSavedBytes / counters.successful
      : 0;

  return (
    <div>
      <Header title="Results Summary" />

      <div className="card">
        <h2>Overview</h2>
        <div className="stats-bar">
          <div className="stat-item">
            <div className="stat-value">{counters.processed}</div>
            <div className="stat-label">Processed</div>
          </div>
          <div className="stat-item">
            <div className="stat-value">{counters.successful}</div>
            <div className="stat-label">Succeeded</div>
          </div>
          <div className="stat-item">
            <div className="stat-value">{counters.failed}</div>
            <div className="stat-label">Failed</div>
          </div>
          <div className="stat-item">
            <div className="stat-value">
              {counters.totalFiles - counters.processed}
            </div>
            <div className="stat-label">Pending</div>
          </div>
        </div>
      </div>

      <div className="card">
        <h2>Savings</h2>
        <div className="stats-bar">
          <div className="stat-item">
            <div className="stat-value">
              {formatBytes(counters.totalSavedBytes, true)}
            </div>
            <div className="stat-label">Total Saved</div>
          </div>
          <div className="stat-item">
            <div className="stat-value">
              {formatBytes(counters.totalMetadataSaved, true)}
            </div>
            <div className="stat-label">Metadata Net</div>
          </div>
          <div className="stat-item">
            <div className="stat-value">
              {formatBytes(counters.totalPaddingSaved, true)}
            </div>
            <div className="stat-label">Padding Trim</div>
          </div>
          <div className="stat-item">
            <div className="stat-value">
              {formatBytes(counters.totalArtworkSaved, true)}
            </div>
            <div className="stat-label">Artwork Net</div>
          </div>
        </div>
      </div>

      <div className="card">
        <h2>Statistics</h2>
        <div className="stats-bar">
          <div className="stat-item">
            <div className="stat-value">{formatPercent(successRate)}</div>
            <div className="stat-label">Success Rate</div>
          </div>
          <div className="stat-item">
            <div className="stat-value">
              {formatBytes(Math.round(avgSaved), true)}
            </div>
            <div className="stat-label">Avg Saved/File</div>
          </div>
          <div className="stat-item">
            <div className="stat-value">{counters.artworkOptimizedFiles}</div>
            <div className="stat-label">Art Files</div>
          </div>
          <div className="stat-item">
            <div className="stat-value">{counters.artworkOptimizedBlocks}</div>
            <div className="stat-label">Art Blocks</div>
          </div>
        </div>
      </div>
    </div>
  );
}
