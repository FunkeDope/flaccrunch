import { WorkerCard } from "./WorkerCard";
import type { WorkerStatus } from "../../types/processing";

interface WorkerGridProps {
  workers: WorkerStatus[];
}

export function WorkerGrid({ workers }: WorkerGridProps) {
  return (
    <div className="worker-grid">
      {workers.map((worker) => (
        <WorkerCard key={worker.id} worker={worker} />
      ))}
    </div>
  );
}
