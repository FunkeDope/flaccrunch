import type { FileEvent } from "../../types/processing";

interface FailedFilesListProps {
  events: FileEvent[];
}

export function FailedFilesList({ events }: FailedFilesListProps) {
  const failed = events.filter((e) => e.status === "FAIL");

  if (failed.length === 0) {
    return <p style={{ color: "var(--text-muted)" }}>No failed files</p>;
  }

  return (
    <div>
      {failed.map((event, i) => (
        <div key={i} className="folder-item">
          <div>
            <div className="path">{event.file}</div>
            <div style={{ fontSize: 12, color: "var(--error)", marginTop: 4 }}>
              {event.detail}
            </div>
          </div>
        </div>
      ))}
    </div>
  );
}
