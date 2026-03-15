import { Header } from "../layout/Header";
import { OverallProgress } from "./OverallProgress";
import { ControlBar } from "./ControlBar";
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
  onCancel: () => void;
}

export function ProcessingDashboard({
  status,
  counters,
  workers,
  recentEvents,
  topCompression,
  onCancel,
}: ProcessingDashboardProps) {
  const isActive = status === "processing" || status === "cancelling";

  return (
    <div>
      <Header title="Processing" />

      <ControlBar
        status={status}
        onCancel={onCancel}
      />

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
    </div>
  );
}
