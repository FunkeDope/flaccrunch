import { getStageColor } from "../../lib/format";
import type { WorkerStatus } from "../../types/processing";

interface WorkerCardProps {
  worker: WorkerStatus;
}

export function WorkerCard({ worker }: WorkerCardProps) {
  return (
    <div className="worker-card">
      <div className="worker-header">
        <span className="worker-id">Worker {worker.id + 1}</span>
        <span className={`worker-stage ${getStageColor(worker.state)}`}>
          {worker.state}
        </span>
      </div>
      <div className="file-name">
        {worker.file ?? "Idle"}
      </div>
      <div className="progress-bar">
        <div
          className="fill"
          style={{ width: `${worker.percent}%` }}
        />
      </div>
      {worker.ratio && (
        <div
          style={{
            fontSize: 11,
            color: "var(--text-muted)",
            marginTop: 4,
            fontFamily: "var(--font-mono)",
          }}
        >
          ratio: {worker.ratio}
        </div>
      )}
    </div>
  );
}
