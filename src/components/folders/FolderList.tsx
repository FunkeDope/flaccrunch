interface FolderListProps {
  folders: string[];
  onRemove: (folder: string) => void;
}

export function FolderList({ folders, onRemove }: FolderListProps) {
  if (folders.length === 0) return null;

  return (
    <ul className="folder-list">
      {folders.map((folder) => (
        <li key={folder} className="folder-item">
          <span className="path">{folder}</span>
          <button
            className="remove-btn"
            onClick={() => onRemove(folder)}
            title="Remove"
          >
            x
          </button>
        </li>
      ))}
    </ul>
  );
}
