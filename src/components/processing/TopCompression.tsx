import { formatBytes, formatPercent } from "../../lib/format";
import type { CompressionResult } from "../../types/processing";

interface TopCompressionProps {
  results: CompressionResult[];
  successCount: number;
}

export function TopCompression({ results, successCount }: TopCompressionProps) {
  return (
    <div className="top-compression-section">
      <div className="top-compression-header">Top 3 Compression (live)</div>
      {results.length === 0 ? (
        <div className="top-compression-empty">
          {successCount > 0
            ? "(No net-positive file reductions yet)"
            : "(No successful file conversions yet)"}
        </div>
      ) : (
        <div className="top-compression-list">
          {results.map((result, i) => {
            const fileName = result.path.split(/[/\\]/).pop() ?? result.path;
            return (
              <div key={i} className="top-compression-entry" title={result.path}>
                <span className="top-compression-rank">{i + 1}.</span>
                <span className="top-compression-detail">
                  Saved {formatBytes(result.savedBytes, true)} ({formatPercent(result.savedPct)})
                </span>
                <span className="top-compression-sep">|</span>
                <span className="top-compression-file">{fileName}</span>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
