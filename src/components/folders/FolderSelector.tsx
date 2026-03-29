import { useState, useEffect } from "react";
import * as api from "../../lib/tauri";
import type { RunStatus } from "../../types/processing";

interface FolderSelectorProps {
  folders: string[];
  onAddFolder: () => void;
  onAddFiles: () => void;
  onRemoveFolder: (folder: string) => void;
  onStart: () => void;
  onTestStorage: () => void;
  canStart: boolean;
  status: RunStatus;
  error: string | null;
  isDragOver?: boolean;
  isDragOverFiles?: boolean;
}

export function FolderSelector({
  folders,
  onAddFolder,
  onAddFiles,
  onRemoveFolder,
  onStart,
  onTestStorage,
  canStart,
  status,
  error,
  isDragOver: nativeDragOver,
}: FolderSelectorProps) {
  const [localFolderDragOver, setLocalFolderDragOver] = useState(false);
  const [localFileDragOver, setLocalFileDragOver] = useState(false);
  const [mobile, setMobile] = useState(false);
  const [showQueue, setShowQueue] = useState(false);

  // Native OS drag highlights both zones (we can't detect file vs folder until drop)
  const folderDragOver = nativeDragOver || localFolderDragOver;
  const fileDragOver = nativeDragOver || localFileDragOver;

  useEffect(() => {
    api.isMobile().then(setMobile).catch(() => setMobile(false));
  }, []);

  const isActive = status === "processing" || status === "cancelling" || status === "complete";

  // During/after processing: show a compact "add more" strip
  if (isActive) {
    return (
      <>
        {error && <div className="error-banner">{error}</div>}
      </>
    );
  }

  // Idle state: full UI
  return (
    <fieldset className="folder-section start-fieldset">
      <legend>Source Selection</legend>
      {error && <div className="error-banner">{error}</div>}
      <div className="start-screen-copy">
        <p>Select FLAC folders or files from the toolbar, then continue.</p>
        <p>
          {folders.length === 0
            ? "No items selected."
            : `${folders.length} ${folders.length === 1 ? "item" : "items"} selected.`}
        </p>
      </div>

      {mobile ? (
        <div className="drop-zone sunken-panel drop-zone-mobile" onClick={onAddFiles}>
          <strong className="drop-zone-title">Tap to add FLAC files</strong>
        </div>
      ) : (
        <div className="drop-zone-row intake-drop-grid">
          <div
            className={`drop-zone sunken-panel ${folderDragOver ? "active" : ""}`}
            onClick={onAddFolder}
            onDragOver={(e) => { e.preventDefault(); setLocalFolderDragOver(true); }}
            onDragLeave={() => setLocalFolderDragOver(false)}
            onDrop={(e) => { e.preventDefault(); setLocalFolderDragOver(false); }}
          >
            <strong className="drop-zone-title">Drop folders</strong>
            <span className="drop-zone-note">or click to browse</span>
          </div>
          <div
            className={`drop-zone sunken-panel ${fileDragOver ? "active" : ""}`}
            onClick={onAddFiles}
            onDragOver={(e) => { e.preventDefault(); setLocalFileDragOver(true); }}
            onDragLeave={() => setLocalFileDragOver(false)}
            onDrop={(e) => { e.preventDefault(); setLocalFileDragOver(false); }}
          >
            <strong className="drop-zone-title">Drop files</strong>
            <span className="drop-zone-note">or click to browse</span>
          </div>
        </div>
      )}

      <div className="intake-footer" style={mobile ? { marginTop: 20 } : undefined}>
        {folders.length > 0 && (
          <>
            <div className="field-row queue-toggle-row">
              <button
                type="button"
                className="queue-peek"
                aria-expanded={showQueue}
                onClick={() => setShowQueue((value) => !value)}
              >
                {showQueue ? "Hide queue" : `Show queue (${folders.length})`}
              </button>
            </div>

            {showQueue && (
              <div className="sunken-panel queue-list-panel">
                <ul className="folder-list">
                  {folders.map((folder) => (
                    <li key={folder} className="folder-item">
                      <span className="path" title={folder}>
                        {mobile ? (folder.split("/").pop() || folder) : folder}
                      </span>
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
          </>
        )}
        <div className="start-footer-actions">
          {mobile && (
            <button onClick={onTestStorage}>Test Storage</button>
          )}
          <button className="default start-next-button" onClick={onStart} disabled={!canStart}>
            Start &gt;
          </button>
        </div>
      </div>
    </fieldset>
  );
}
