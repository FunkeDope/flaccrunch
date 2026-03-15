import { formatBytes, formatPercent } from "../../lib/format";
import type { CompressionResult } from "../../types/processing";

interface TopCompressionProps {
  results: CompressionResult[];
}

export function TopCompression({ results }: TopCompressionProps) {
  return (
    <div className="top-compression">
      {results.map((result, i) => (
        <div key={i} className="rank">
          <span className="rank-number">{i + 1}</span>
          <span className="rank-file" title={result.path}>
            {result.path.split("/").pop() ?? result.path}
          </span>
          <span className="rank-saved">
            {formatBytes(result.savedBytes, true)} ({formatPercent(result.savedPct)})
          </span>
        </div>
      ))}
    </div>
  );
}
