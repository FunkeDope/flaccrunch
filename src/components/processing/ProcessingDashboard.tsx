import { WorkerGrid } from "./WorkerGrid";
import { RecentEventsTable } from "./RecentEventsTable";
import { TopCompression } from "./TopCompression";
import type {
  WorkerStatus,
  FileEvent,
} from "../../types/processing";

interface ProcessingDashboardProps {
  workers: WorkerStatus[];
  recentEvents: FileEvent[];
  topCompression: FileEvent[];
}

export function ProcessingDashboard({
  workers,
  recentEvents,
  topCompression,
}: ProcessingDashboardProps) {
  return (
    <div className="processing-section">
      <TopCompression results={topCompression} />

      {workers.length > 0 && (
        <div className="card">
          <h2>Workers</h2>
          <WorkerGrid workers={workers} />
        </div>
      )}

      {recentEvents.length > 0 && (
        <div className="card files-card">
          <div className="card-header">
            <h2>Files</h2>
          </div>
          <RecentEventsTable events={recentEvents} />
        </div>
      )}
    </div>
  );
}
