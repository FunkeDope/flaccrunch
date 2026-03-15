import { RecentEventsTable } from "./RecentEventsTable";
import type { FileEvent } from "../../types/processing";

interface TopCompressionProps {
  results: FileEvent[];
}

export function TopCompression({ results }: TopCompressionProps) {
  return (
    <div className="top-compression-section">
      <div className="top-compression-header">Top Compression</div>
      <RecentEventsTable events={results} maxRows={3} minRows={3} defaultSortKey="audio" />
    </div>
  );
}
