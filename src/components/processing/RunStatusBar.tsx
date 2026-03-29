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
  const totalSavedPct =
    counters.totalOriginalBytes > 0
      ? (counters.totalSavedBytes / counters.totalOriginalBytes) * 100
      : 0;

  return (
    <div className={`window run-status-window ${barClass}`}>
      <div className="window-body run-status-body">
        <div className="run-status-main">
          <span className={`status-bar-label ${labelClass}`}>{labelText}</span>
          <div className="progress-indicator segmented run-progress">
            <span
              className="progress-indicator-bar"
              style={{ width: `${barPct}%` }}
            />
          </div>
          <span className="status-bar-pct">{pct}%</span>
          <div className="status-bar-actions">
            {isRunning && (
              <button onClick={onCancel} disabled={status === "cancelling"}>
                {status === "cancelling" ? "Cancelling…" : "Cancel"}
              </button>
            )}
            {isComplete && (
              <>
                <button onClick={onExport} title="Export log as text file">
                  Export Log
                </button>
                <button className="default" onClick={onReset}>
                  New Run
                </button>
              </>
            )}
          </div>
        </div>
      </div>

      <div className="status-bar">
        {counters.totalSavedBytes > 0 && (
          <p className="status-bar-field">
            Saved {formatBytes(counters.totalSavedBytes)}
            {counters.totalOriginalBytes > 0 ? ` (${totalSavedPct.toFixed(1)}%)` : ""}
          </p>
        )}
        <p className="status-bar-field">
          {counters.processed}/{counters.totalFiles} files
        </p>
        {isRunning && startTime && (
          <p className="status-bar-field">{formatElapsed(elapsed)}</p>
        )}
        {isComplete && startTime && (
          <p className="status-bar-field">
            {formatElapsed(Math.floor((Date.now() - startTime) / 1000))}
          </p>
        )}
        <p className="status-bar-field chip-success">
          OK <strong>{counters.successful}</strong>
        </p>
        {counters.failed > 0 && (
          <p className="status-bar-field chip-error">
            Fail <strong>{counters.failed}</strong>
          </p>
        )}
        {counters.totalSavedBytes > 0 && (
          <p className="status-bar-field">
            Audio {formatBytes(audioSaved > 0 ? audioSaved : counters.totalSavedBytes)}
          </p>
        )}
        {counters.totalArtworkSaved > 0 && (
          <p className="status-bar-field">Art {formatBytes(counters.totalArtworkSaved)}</p>
        )}
        {metaSaved > 0 && (
          <p className="status-bar-field">Meta {formatBytes(metaSaved)}</p>
        )}
        {isComplete && counters.totalFiles > 0 && (
          <p className="status-bar-field">
            Rate {((counters.successful / counters.totalFiles) * 100).toFixed(1)}%
          </p>
        )}
        {isComplete && counters.artworkOptimizedFiles > 0 && (
          <p className="status-bar-field">Art files {counters.artworkOptimizedFiles}</p>
        )}
      </div>
    </div>
  );
}
