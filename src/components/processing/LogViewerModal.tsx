import { useState } from "react";

interface LogViewerModalProps {
  text: string;
  loading: boolean;
  error: string | null;
  onSave: () => void;
  onClose: () => void;
}

export function LogViewerModal({ text, loading, error, onSave, onClose }: LogViewerModalProps) {
  const [maximized, setMaximized] = useState(false);

  return (
    <div
      className="modal-overlay"
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div className={`window modal log-viewer-window ${maximized ? "is-maximized" : ""}`}>
        <div className="title-bar">
          <div className="title-bar-text">Full Log</div>
          <div className="title-bar-controls">
            <button
              aria-label={maximized ? "Restore" : "Maximize"}
              onClick={() => setMaximized((value) => !value)}
            />
            <button aria-label="Close" onClick={onClose} />
          </div>
        </div>
        <div className="window-body modal-body log-viewer-body">
          <div className="sunken-panel log-viewer-shell">
            {loading ? (
              <div className="log-viewer-text log-viewer-placeholder">Loading final log...</div>
            ) : (
              <pre className="log-viewer-text">{error ?? text}</pre>
            )}
          </div>
          <div className="field-row settings-actions">
            <button onClick={onSave} disabled={loading || !!error}>Save Log...</button>
            <button className="default" onClick={onClose}>Close</button>
          </div>
        </div>
      </div>
    </div>
  );
}
