import { getStageColor } from "../../lib/format";
import type { WorkerStatus } from "../../types/processing";

interface WorkerCardProps {
  worker: WorkerStatus;
}

export function WorkerCard({ worker }: WorkerCardProps) {
  const isActive = worker.state !== "idle";
  const showPercent = worker.state === "converting" && worker.percent > 0;

  return (
    <div className={`worker-card ${isActive ? "active" : ""}`}>
      <div className="worker-header">
        <span className="worker-id">Worker {worker.id + 1}</span>
        <span className={`worker-stage ${getStageColor(worker.state)}`}>
          {worker.state}
        </span>
      </div>
      <div className="file-name">
        {worker.file
          ? (worker.file.split("/").pop() ?? worker.file)
          : "Idle"}
      </div>
      {isActive && (
        <>
          <div className="worker-progress-row">
            <div className="progress-bar">
              <div
                className="fill"
                style={{ width: `${worker.percent}%` }}
              />
            </div>
            {showPercent && (
              <span className="worker-percent">{worker.percent}%</span>
            )}
          </div>
          {worker.ratio && (
            <div className="worker-ratio">
              ratio: {worker.ratio}
            </div>
          )}
        </>
      )}
    </div>
  );
}
