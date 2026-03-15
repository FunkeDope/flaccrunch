interface FolderItemProps {
  path: string;
  onRemove: () => void;
}

export function FolderItem({ path, onRemove }: FolderItemProps) {
  return (
    <li className="folder-item">
      <span className="path">{path}</span>
      <button className="remove-btn" onClick={onRemove} title="Remove">
        x
      </button>
    </li>
  );
}
