import { useState, useEffect } from "react";
import * as api from "../../lib/tauri";
import type { RunStatus } from "../../types/processing";

interface FolderSelectorProps {
  folders: string[];
  onAddFolder: () => void;
  onAddFiles: () => void;
  onRemoveFolder: (folder: string) => void;
  onStart: () => void;
  canStart: boolean;
  status: RunStatus;
  error: string | null;
  isDragOver?: boolean;
}

export function FolderSelector({
  folders,
  onAddFolder,
  onAddFiles,
  onRemoveFolder,
  onStart,
  canStart,
  status,
  error,
  isDragOver: nativeDragOver,
}: FolderSelectorProps) {
  const [localDragOver, setLocalDragOver] = useState(false);
  const [mobile, setMobile] = useState(false);

  // Combine native OS drag (from Tauri) and local HTML5 drag state
  const dragOver = nativeDragOver || localDragOver;

  useEffect(() => {
    api.isMobile().then(setMobile).catch(() => setMobile(false));
  }, []);

  const isActive = status === "processing" || status === "cancelling" || status === "complete";

  // During/after processing: show a compact "add more" strip
  if (isActive) {
    const queueLabel =
      folders.length === 0
        ? "Queue additional files while processing"
        : folders.length === 1
        ? folders[0]
        : `${folders[0]}  (+${folders.length - 1} more)`;

    return (
      <div className="folder-section">
        {error && <div className="error-banner">{error}</div>}
        <div className="add-more-strip">
          <span className="queue-label" title={folders.join("\n")}>
            {queueLabel}
          </span>
          {mobile ? (
            <button className="btn btn-secondary" style={{ padding: "5px 12px", minHeight: 30, fontSize: 12 }} onClick={onAddFiles}>
              + Files
            </button>
          ) : (
            <>
              <button className="btn btn-secondary" style={{ padding: "5px 12px", minHeight: 30, fontSize: 12 }} onClick={onAddFolder}>
                + Folder
              </button>
              <button className="btn btn-secondary" style={{ padding: "5px 12px", minHeight: 30, fontSize: 12 }} onClick={onAddFiles}>
                + Files
              </button>
            </>
          )}
        </div>
      </div>
    );
  }

  // Idle state: full UI
  return (
    <div className="folder-section">
      {error && <div className="error-banner">{error}</div>}

      {mobile ? (
        <div className="drop-zone" onClick={onAddFiles}>
          Tap to select FLAC files
        </div>
      ) : (
        <div className="drop-zone-row">
          <div
            className={`drop-zone ${dragOver ? "active" : ""}`}
            onClick={onAddFolder}
            onDragOver={(e) => { e.preventDefault(); setLocalDragOver(true); }}
            onDragLeave={() => setLocalDragOver(false)}
            onDrop={(e) => { e.preventDefault(); setLocalDragOver(false); }}
          >
            Drop folders here or click to browse
          </div>
          <div className="drop-zone" onClick={onAddFiles}>
            Click to add individual FLAC files
          </div>
        </div>
      )}

      {folders.length > 0 && (
        <div className="card">
          <div className="card-header">
            <h2>Selected ({folders.length})</h2>
          </div>
          <ul className="folder-list">
            {folders.map((folder) => (
              <li key={folder} className="folder-item">
                <span className="path">{folder}</span>
                <button
                  className="remove-btn"
                  onClick={() => onRemoveFolder(folder)}
                  title="Remove"
                >
                  ×
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}

      <div className="action-bar">
        <button
          className="btn btn-primary btn-full"
          onClick={onStart}
          disabled={!canStart}
        >
          Start Processing
        </button>
      </div>
    </div>
  );
}
