import { useState, useCallback } from "react";
import { Header } from "../layout/Header";

interface FolderSelectorProps {
  folders: string[];
  onAddFolder: () => void;
  onRemoveFolder: (folder: string) => void;
  onStart: () => void;
  canStart: boolean;
}

export function FolderSelector({
  folders,
  onAddFolder,
  onRemoveFolder,
  onStart,
  canStart,
}: FolderSelectorProps) {
  const [dragOver, setDragOver] = useState(false);

  const handleDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      setDragOver(false);
      // Drag-and-drop folder handling would go here
      // For now, trigger the native dialog
      onAddFolder();
    },
    [onAddFolder]
  );

  return (
    <div>
      <Header title="Select Folders" />

      <div
        className={`drop-zone ${dragOver ? "active" : ""}`}
        onClick={onAddFolder}
        onDragOver={(e) => {
          e.preventDefault();
          setDragOver(true);
        }}
        onDragLeave={() => setDragOver(false)}
        onDrop={handleDrop}
      >
        Click to browse or drag folders here
      </div>

      {folders.length > 0 && (
        <div className="card">
          <h2>Selected Folders ({folders.length})</h2>
          <ul className="folder-list">
            {folders.map((folder) => (
              <li key={folder} className="folder-item">
                <span className="path">{folder}</span>
                <button
                  className="remove-btn"
                  onClick={() => onRemoveFolder(folder)}
                  title="Remove folder"
                >
                  x
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}

      <button
        className="btn btn-primary"
        onClick={onStart}
        disabled={!canStart}
      >
        Start Processing
      </button>
    </div>
  );
}
