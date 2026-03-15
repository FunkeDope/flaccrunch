import { useState, useCallback } from "react";
import type { RunStatus } from "../../types/processing";

interface FolderSelectorProps {
  folders: string[];
  onAddFolder: () => void;
  onRemoveFolder: (folder: string) => void;
  onStart: () => void;
  onCancel: () => void;
  onReset: () => void;
  canStart: boolean;
  status: RunStatus;
  error: string | null;
}

export function FolderSelector({
  folders,
  onAddFolder,
  onRemoveFolder,
  onStart,
  onCancel,
  onReset,
  canStart,
  status,
  error,
}: FolderSelectorProps) {
  const [dragOver, setDragOver] = useState(false);

  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      setDragOver(false);
      onAddFolder();
    },
    [onAddFolder]
  );

  const isProcessing = status === "processing" || status === "cancelling";
  const isComplete = status === "complete";

  return (
    <div className="folder-section">
      {error && (
        <div className="error-banner">
          {error}
        </div>
      )}

      <div
        className={`drop-zone ${dragOver ? "active" : ""} ${isProcessing ? "disabled" : ""}`}
        onClick={isProcessing ? undefined : onAddFolder}
        onDragOver={(e) => {
          e.preventDefault();
          setDragOver(true);
        }}
        onDragLeave={() => setDragOver(false)}
        onDrop={handleDrop}
      >
        Click to select FLAC files
      </div>

      {folders.length > 0 && (
        <div className="card">
          <div className="card-header">
            <h2>Selected ({folders.length})</h2>
          </div>
          <ul className="folder-list">
            {folders.map((folder) => (
              <li key={folder} className="folder-item">
                <span className="path">{folder}</span>
                {!isProcessing && (
                  <button
                    className="remove-btn"
                    onClick={() => onRemoveFolder(folder)}
                    title="Remove folder"
                  >
                    x
                  </button>
                )}
              </li>
            ))}
          </ul>
        </div>
      )}

      <div className="action-bar">
        {isComplete ? (
          <button className="btn btn-primary" onClick={onReset}>
            New Run
          </button>
        ) : isProcessing ? (
          <button
            className="btn btn-danger"
            onClick={onCancel}
            disabled={status === "cancelling"}
          >
            {status === "cancelling" ? "Cancelling..." : "Cancel"}
          </button>
        ) : (
          <button
            className="btn btn-primary"
            onClick={onStart}
            disabled={!canStart}
          >
            Start Processing
          </button>
        )}
      </div>
    </div>
  );
}
