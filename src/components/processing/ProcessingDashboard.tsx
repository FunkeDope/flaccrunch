import { OverallProgress } from "./OverallProgress";
import { StatsBar } from "./StatsBar";
import { WorkerGrid } from "./WorkerGrid";
import { RecentEventsTable } from "./RecentEventsTable";
import { TopCompression } from "./TopCompression";
import type {
  RunStatus,
  RunCounters,
  WorkerStatus,
  FileEvent,
  CompressionResult,
} from "../../types/processing";

interface ProcessingDashboardProps {
  status: RunStatus;
  counters: RunCounters;
  workers: WorkerStatus[];
  recentEvents: FileEvent[];
  topCompression: CompressionResult[];
}

export function ProcessingDashboard({
  status,
  counters,
  workers,
  recentEvents,
  topCompression,
}: ProcessingDashboardProps) {
  return (
    <div className="processing-section">
      <OverallProgress counters={counters} />

      <StatsBar counters={counters} />

      {workers.length > 0 && (
        <div className="card">
          <h2>Workers</h2>
          <WorkerGrid workers={workers} />
        </div>
      )}

      {topCompression.length > 0 && (
        <div className="card">
          <h2>Top Compression</h2>
          <TopCompression results={topCompression} />
        </div>
      )}

      {recentEvents.length > 0 && (
        <div className="card">
          <h2>Recent Files</h2>
          <RecentEventsTable events={recentEvents} />
        </div>
      )}

      {status === "complete" && counters.processed > 0 && (
        <div className="card">
          <h2>Summary</h2>
          <div className="stats-bar">
            <div className="stat-item">
              <div className="stat-value">
                {counters.totalFiles > 0
                  ? ((counters.successful / counters.totalFiles) * 100).toFixed(1) + "%"
                  : "0%"}
              </div>
              <div className="stat-label">Success Rate</div>
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
      )}
    </div>
  );
}
