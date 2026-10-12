import { useState, useRef, useCallback, useMemo } from "react";
import { WorkerGrid } from "./WorkerGrid";
import { WorkerTable } from "./WorkerTable";
import { RecentEventsTable } from "./RecentEventsTable";
import { failedFolders } from "../../lib/format";
import type {
  WorkerStatus,
  FileEvent,
} from "../../types/processing";

interface ProcessingDashboardProps {
  workers: WorkerStatus[];
  recentEvents: FileEvent[];
  runFolders: string[];
  /** How the active theme presents workers: sortable table or themed cards. */
  workerLayout?: "cards" | "table";
}

export function ProcessingDashboard({
  workers,
  recentEvents,
  runFolders,
  workerLayout = "cards",
}: ProcessingDashboardProps) {
  const [workersCollapsed, setWorkersCollapsed] = useState(false);
  // null = automatic: the workers pane takes the height its cards need (capped
  // by CSS) and the files table gets the rest. Dragging the handle sets a ratio.
  const [splitRatio, setSplitRatio] = useState<number | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const badFolders = useMemo(() => failedFolders(recentEvents), [recentEvents]);
  const failedCount = badFolders.reduce((sum, f) => sum + f.count, 0);

  const handleDragStart = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    const container = containerRef.current;
    if (!container) return;

    document.body.style.cursor = "row-resize";
    document.body.style.userSelect = "none";

    const onMouseMove = (ev: MouseEvent) => {
      const rect = container.getBoundingClientRect();
      const ratio = (ev.clientY - rect.top) / rect.height;
      setSplitRatio(Math.max(0.1, Math.min(0.85, ratio)));
    };

    const onMouseUp = () => {
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
      document.removeEventListener("mousemove", onMouseMove);
      document.removeEventListener("mouseup", onMouseUp);
    };

    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp);
  }, []);

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

  const showWorkers = workers.length > 0;
  const gridRows = !showWorkers
    ? "minmax(0, 1fr)"
    : splitRatio === null
      ? "auto auto minmax(0, 1fr)"
      : `minmax(0, ${splitRatio}fr) auto minmax(0, ${1 - splitRatio}fr)`;

  return (
    <div
      className="processing-section"
      ref={containerRef}
      style={{ gridTemplateRows: gridRows }}
    >
      {showWorkers && (
        <fieldset className={`workers-pane ${splitRatio === null ? "is-auto" : ""}`}>
          <legend>Workers</legend>
          <div className="pane-toolbar">
            <div className="workers-heading">
              <span className="section-kicker">
                {workers.length} slots · {runFolders.length} {runFolders.length === 1 ? "source" : "sources"}
              </span>
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
              {workerLayout === "table" ? <WorkerTable workers={workers} /> : <WorkerGrid workers={workers} />}
            </div>
          )}
        </fieldset>
      )}

      {showWorkers && (
        <div
          className="resize-handle"
          onMouseDown={handleDragStart}
          onDoubleClick={() => setSplitRatio(null)}
          title="Drag to resize; double-click to fit the workers pane to its content"
        />
      )}

      <fieldset className="files-pane">
        <legend>Files</legend>
        <div className="pane-toolbar">
          <div className="workers-heading">
            <span className="section-kicker">{recentEvents.length} results</span>
            {failedCount > 0 && (
              <span className="bad-files-kicker">
                {failedCount} bad file{failedCount === 1 ? "" : "s"} in {badFolders.length} folder
                {badFolders.length === 1 ? "" : "s"}
              </span>
            )}
          </div>
        </div>
        {badFolders.length > 0 && (
          <div className="sunken-panel bad-folders-panel" data-testid="bad-folders">
            <div className="queue-section-label">Folders with bad files ({badFolders.length})</div>
            <ul className="folder-list">
              {badFolders.map(({ folder, count }) => (
                <li key={folder} className="folder-item">
                  <span className="path" title={folder}>{folder}</span>
                  <span className="bad-folder-count">{count}</span>
                </li>
              ))}
            </ul>
          </div>
        )}
        <RecentEventsTable events={recentEvents} defaultSortKey="pct" />
      </fieldset>
    </div>
  );
}
