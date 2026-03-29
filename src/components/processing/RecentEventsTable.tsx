import { useState, useMemo } from "react";
import { formatBytes, getStatusColor, compressionRowClass } from "../../lib/format";
import type { FileEvent } from "../../types/processing";

interface RecentEventsTableProps {
  events: FileEvent[];
  /** When set, limits visible rows and hides the expand button. */
  maxRows?: number;
  /** When set, always renders at least this many rows (empty placeholders). */
  minRows?: number;
  /** Override the initial sort column (default: "time"). */
  defaultSortKey?: SortKey;
  /** Override the initial sort direction (default: "desc"). */
  defaultSortDir?: SortDir;
}

type SortKey = "time" | "file" | "audio" | "art" | "pct" | "verify" | "status";
type SortDir = "asc" | "desc";

function SortHeader({
  label,
  col,
  current,
  dir,
  onSort,
}: {
  label: string;
  col: SortKey;
  current: SortKey;
  dir: SortDir;
  onSort: (col: SortKey) => void;
}) {
  const active = current === col;
  return (
    <th className={`sortable-th${active ? " sort-active" : ""}`} onClick={() => onSort(col)}>
      {label}
      <span className="sort-arrow">{active ? (dir === "asc" ? " ▲" : " ▼") : " ⇅"}</span>
    </th>
  );
}

export function RecentEventsTable({ events, maxRows, minRows, defaultSortKey = "time", defaultSortDir = "desc" }: RecentEventsTableProps) {
  const [sortKey, setSortKey] = useState<SortKey>(defaultSortKey);
  const [sortDir, setSortDir] = useState<SortDir>(defaultSortDir);

  const handleSort = (col: SortKey) => {
    if (col === sortKey) {
      setSortDir((d) => (d === "asc" ? "desc" : "asc"));
    } else {
      setSortKey(col);
      // Most columns: desc by default (largest first). Time desc = newest first.
      setSortDir(col === "file" || col === "verify" || col === "status" ? "asc" : "desc");
    }
  };

  const sorted = useMemo(() => {
    const copy = [...events].reverse(); // newest first base order
    const mul = sortDir === "asc" ? 1 : -1;

    copy.sort((a, b) => {
      switch (sortKey) {
        case "time":
          // Keep original insertion order (reversed = newest first for desc)
          return 0; // already in correct order from .reverse() for desc; stable sort preserves it
        case "file": {
          const fa = (a.file.split(/[/\\]/).pop() ?? a.file).toLowerCase();
          const fb = (b.file.split(/[/\\]/).pop() ?? b.file).toLowerCase();
          return mul * fa.localeCompare(fb);
        }
        case "audio":
          return mul * (a.savedBytes - b.savedBytes);
        case "art":
          return mul * (a.artworkSavedBytes - b.artworkSavedBytes);
        case "pct":
          return mul * (a.compressionPct - b.compressionPct);
        case "verify": {
          const va = (a.verification || a.detail || "").toLowerCase();
          const vb = (b.verification || b.detail || "").toLowerCase();
          return mul * va.localeCompare(vb);
        }
        case "status":
          return mul * a.status.localeCompare(b.status);
        default:
          return 0;
      }
    });

    // For "time" sort, just respect the direction by reversing the already-newest-first copy
    if (sortKey === "time" && sortDir === "asc") {
      copy.reverse();
    }

    return copy;
  }, [events, sortKey, sortDir]);

  const limit = maxRows ?? Infinity;
  const visible = sorted.slice(0, limit);

  return (
    <div className="events-table-shell">
      <div className="sunken-panel events-table-wrap">
        <table className="events-table interactive">
          <thead>
            <tr>
              <SortHeader label="Time"   col="time"   current={sortKey} dir={sortDir} onSort={handleSort} />
              <SortHeader label="St"     col="status" current={sortKey} dir={sortDir} onSort={handleSort} />
              <SortHeader label="File"   col="file"   current={sortKey} dir={sortDir} onSort={handleSort} />
              <SortHeader label="Audio"  col="audio"  current={sortKey} dir={sortDir} onSort={handleSort} />
              <SortHeader label="Art"    col="art"    current={sortKey} dir={sortDir} onSort={handleSort} />
              <SortHeader label="%"      col="pct"    current={sortKey} dir={sortDir} onSort={handleSort} />
              <SortHeader label="Verify" col="verify" current={sortKey} dir={sortDir} onSort={handleSort} />
            </tr>
          </thead>
          <tbody>
            {visible.map((event, i) => {
              const rowClass = compressionRowClass(event.compressionPct);
              return (
                <tr key={i} className={rowClass}>
                  <td>{event.time}</td>
                  <td className={getStatusColor(event.status)}>{event.status}</td>
                  <td
                    style={{ maxWidth: 220, overflow: "hidden", textOverflow: "ellipsis" }}
                    title={event.file}
                  >
                    {event.file.split(/[/\\]/).pop() ?? event.file}
                  </td>
                  <td>
                    {event.savedBytes > 0 ? (
                      <span className="saved-audio">{formatBytes(event.savedBytes)}</span>
                    ) : (
                      <span className="saved-zero">—</span>
                    )}
                  </td>
                  <td>
                    {event.artworkSavedBytes > 0 ? (
                      <span className="saved-art">{formatBytes(event.artworkSavedBytes)}</span>
                    ) : (
                      <span className="saved-zero">—</span>
                    )}
                  </td>
                  <td>
                    {event.compressionPct > 0 ? (
                      <span style={{ color: event.compressionPct >= 10 ? "var(--success)" : event.compressionPct >= 5 ? "var(--warning)" : "var(--text-muted)" }}>
                        {event.compressionPct.toFixed(1)}%
                      </span>
                    ) : (
                      <span className="saved-zero">0%</span>
                    )}
                  </td>
                  <td style={{ maxWidth: 140, overflow: "hidden", textOverflow: "ellipsis" }}>
                    {(() => {
                      const v = (event.verification || event.detail || "").toLowerCase();
                      const color = v.startsWith("mismatch") || v.startsWith("fail")
                        ? "var(--error)"
                        : v.startsWith("match") || v === "ok"
                        ? "var(--success)"
                        : v.includes("warn") || v.includes("error")
                        ? "var(--warning)"
                        : "var(--text-muted)";
                      return <span style={{ color }}>{event.verification || event.detail || "—"}</span>;
                    })()}
                  </td>
                </tr>
              );
            })}
            {minRows !== undefined &&
              Array.from({ length: Math.max(0, minRows - visible.length) }).map((_, i) => (
                <tr key={`placeholder-${i}`} className="placeholder-row">
                  <td colSpan={7} style={{ color: "var(--text-muted)", textAlign: "center" }}>—</td>
                </tr>
              ))}
          </tbody>
        </table>
      </div>
      <div className="events-table-footer">
        <span>{sorted.length} total files</span>
      </div>
    </div>
  );
}
