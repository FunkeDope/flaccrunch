import { useState } from "react";
import { WorkerGrid } from "./WorkerGrid";
import { RecentEventsTable } from "./RecentEventsTable";
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
}: ProcessingDashboardProps) {
  const [workersCollapsed, setWorkersCollapsed] = useState(false);

  return (
    <div className="processing-section">
      {workers.length > 0 && (
        <fieldset className="workers-pane">
          <legend>Workers</legend>
          <div className="pane-toolbar">
            <div className="workers-heading">
              <span className="section-kicker">{workers.length} slots</span>
            </div>
            <button
              type="button"
              className="worker-toggle"
              aria-expanded={!workersCollapsed}
              onClick={() => setWorkersCollapsed((value) => !value)}
            >
              {workersCollapsed ? "Expand" : "Collapse"}
            </button>
          </div>
          {!workersCollapsed && (
            <div className="sunken-panel worker-body">
              <WorkerGrid workers={workers} />
            </div>
          )}
        </fieldset>
      )}

      <fieldset className="files-pane">
        <legend>Files</legend>
        <div className="pane-toolbar">
          <div className="workers-heading">
            <span className="section-kicker">{recentEvents.length} results</span>
          </div>
        </div>
        <RecentEventsTable events={recentEvents} defaultSortKey="pct" />
      </fieldset>
    </div>
  );
}
