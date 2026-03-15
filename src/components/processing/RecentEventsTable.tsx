import { formatBytes, getStatusColor } from "../../lib/format";
import type { FileEvent } from "../../types/processing";

interface RecentEventsTableProps {
  events: FileEvent[];
}

export function RecentEventsTable({ events }: RecentEventsTableProps) {
  return (
    <div style={{ overflowX: "auto" }}>
      <table className="events-table">
        <thead>
          <tr>
            <th>Time</th>
            <th>Status</th>
            <th>File</th>
            <th>Saved</th>
            <th>Verification</th>
          </tr>
        </thead>
        <tbody>
          {[...events].reverse().map((event, i) => (
            <tr key={i}>
              <td>{event.time}</td>
              <td className={getStatusColor(event.status)}>{event.status}</td>
              <td
                style={{
                  maxWidth: 300,
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                }}
                title={event.file}
              >
                {event.file.split(/[/\\]/).pop() ?? event.file}
              </td>
              <td>{formatBytes(event.savedBytes, true)}</td>
              <td>{event.verification}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
