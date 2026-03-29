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

  const convertingCount = workers.filter((worker) => worker.state === "converting").length;
  const hashingCount = workers.filter(
    (worker) => worker.state === "hashing-source" || worker.state === "hashing-output"
  ).length;
  const artworkCount = workers.filter((worker) => worker.state === "artwork").length;
  const finalizingCount = workers.filter((worker) => worker.state === "finalizing").length;
  const activeCount = workers.filter((worker) => worker.state !== "idle").length;
  const idleCount = workers.length - activeCount;

  const workerSummaryParts = [
    convertingCount > 0 ? `${convertingCount} converting` : null,
    hashingCount > 0 ? `${hashingCount} hashing` : null,
    artworkCount > 0 ? `${artworkCount} art` : null,
    finalizingCount > 0 ? `${finalizingCount} finalizing` : null,
    idleCount > 0 ? `${idleCount} idle` : null,
  ].filter(Boolean);

  const workerSummary = workerSummaryParts.length > 0
    ? workerSummaryParts.join(" | ")
    : "waiting for work";

  return (
    <div className="processing-section">
      {workers.length > 0 && (
        <fieldset className="workers-pane">
          <legend>Workers</legend>
          <div className="pane-toolbar">
            <div className="workers-heading">
              <span className="section-kicker">{workers.length} slots</span>
              <span className="worker-summary">
                {workersCollapsed ? `${activeCount} active | ${workerSummary}` : workerSummary}
              </span>
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
