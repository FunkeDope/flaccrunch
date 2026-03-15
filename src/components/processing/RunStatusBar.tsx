import { useState, useEffect } from "react";
import { formatBytes, formatElapsed } from "../../lib/format";
import type { RunStatus, RunCounters } from "../../types/processing";

interface RunStatusBarProps {
  status: RunStatus;
  counters: RunCounters;
  startTime: number | null;
  livePct?: number;
  onCancel: () => void;
  onReset: () => void;
  onExport: () => void;
}

function useElapsed(startTime: number | null, running: boolean): number {
  const [elapsed, setElapsed] = useState(0);
  useEffect(() => {
    if (!startTime || !running) return;
    const interval = setInterval(() => {
      setElapsed(Math.floor((Date.now() - startTime) / 1000));
    }, 1000);
    return () => clearInterval(interval);
  }, [startTime, running]);
  return elapsed;
}

export function RunStatusBar({
  status,
  counters,
  startTime,
  livePct,
  onCancel,
  onReset,
  onExport,
}: RunStatusBarProps) {
  const isRunning = status === "processing" || status === "cancelling";
  const isComplete = status === "complete";
  const elapsed = useElapsed(startTime, isRunning);

  // barPct drives the bar width (live, blended with in-flight worker progress).
  // pct is the integer label — always matches the bar so they stay in sync.
  const barPct =
    livePct ??
    (counters.totalFiles > 0
      ? Math.round((counters.processed / counters.totalFiles) * 100)
      : 0);
  const pct = Math.round(barPct);

  // Determine completion color class
  let barClass = "";
  let labelClass = "label-processing";
  let labelText = "Processing";

  if (status === "cancelling") {
    labelClass = "label-cancelling";
    labelText = "Cancelling";
  } else if (isComplete) {
    if (counters.failed === 0) {
      barClass = "status-success";
      labelClass = "label-success";
      labelText = "Complete";
    } else if (counters.successful === 0) {
      barClass = "status-error";
      labelClass = "label-error";
      labelText = "Failed";
    } else {
      barClass = "status-warning";
      labelClass = "label-warning";
      labelText = "Complete (with errors)";
    }
  }

  // Savings breakdown
  const audioSaved = counters.totalSavedBytes - counters.totalArtworkSaved;
  const metaSaved = counters.totalMetadataSaved + counters.totalPaddingSaved;

  return (
    <div className={`run-status-bar ${barClass}`}>
      <div className="status-bar-top">
        <span className={`status-bar-label ${labelClass}`}>{labelText}</span>
        <div className="status-bar-progress-wrap">
          <div className="progress-bar">
            <div
              className={`fill ${isRunning ? "animated" : ""}`}
              style={{
                width: `${barPct}%`,
                background: isComplete
                  ? counters.failed === 0
                    ? "var(--success)"
                    : counters.successful === 0
                    ? "var(--error)"
                    : "var(--warning)"
                  : undefined,
              }}
            />
          </div>
        </div>
        <span className="status-bar-pct">{pct}%</span>
        <div className="status-bar-actions">
          {isRunning && (
            <button
              className="btn btn-danger"
              onClick={onCancel}
              disabled={status === "cancelling"}
              style={{ padding: "5px 12px", minHeight: 30, fontSize: 12 }}
            >
              {status === "cancelling" ? "Cancelling…" : "Cancel"}
            </button>
          )}
          {isComplete && (
            <>
              <button
                className="btn btn-secondary"
                onClick={onExport}
                title="Export log as text file"
                style={{ padding: "5px 12px", minHeight: 30, fontSize: 12 }}
              >
                Export Log
              </button>
              <button
                className="btn btn-primary"
                onClick={onReset}
                style={{ padding: "5px 14px", minHeight: 30, fontSize: 12 }}
              >
                New Run
              </button>
            </>
          )}
        </div>
      </div>

      <div className="status-bar-stats">
        <span className="stat-chip">
          <strong>{counters.processed}</strong> / {counters.totalFiles} files
        </span>
        {isRunning && startTime && (
          <span className="stat-chip">
            {formatElapsed(elapsed)}
          </span>
        )}
        {isComplete && startTime && (
          <span className="stat-chip">
            {formatElapsed(Math.floor((Date.now() - startTime) / 1000))}
          </span>
        )}
        <span className="stat-chip chip-success">
          ✓ <strong>{counters.successful}</strong>
        </span>
        {counters.failed > 0 && (
          <span className="stat-chip chip-error">
            ✗ <strong>{counters.failed}</strong>
          </span>
        )}
        {counters.totalSavedBytes > 0 && (
          <span className="stat-chip chip-saved">
            Audio: <strong>{formatBytes(audioSaved > 0 ? audioSaved : counters.totalSavedBytes)}</strong>
          </span>
        )}
        {counters.totalArtworkSaved > 0 && (
          <span className="stat-chip chip-art">
            Art: <strong>{formatBytes(counters.totalArtworkSaved)}</strong>
          </span>
        )}
        {metaSaved > 0 && (
          <span className="stat-chip">
            Meta: <strong>{formatBytes(metaSaved)}</strong>
          </span>
        )}
        {isComplete && counters.totalFiles > 0 && (
          <span className="stat-chip">
            Rate:{" "}
            <strong>
              {((counters.successful / counters.totalFiles) * 100).toFixed(1)}%
            </strong>
          </span>
        )}
        {isComplete && counters.artworkOptimizedFiles > 0 && (
          <span className="stat-chip chip-art">
            Art files: <strong>{counters.artworkOptimizedFiles}</strong>
          </span>
        )}
      </div>
    </div>
  );
}
