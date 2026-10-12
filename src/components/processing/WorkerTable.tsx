import { useMemo, useState } from "react";
import type { WorkerStatus } from "../../types/processing";
import { getStageColor, getStageLabel } from "../../lib/format";
import { NULL_MD5, displayHash, formatSavedPct, hashColors } from "./WorkerCard";

interface WorkerTableProps {
  workers: WorkerStatus[];
}

type SortKey = "id" | "stage" | "file" | "progress" | "emb" | "pre" | "out" | "saved";
type SortDir = "asc" | "desc";

function SortHeader({
  label,
  col,
  current,
  dir,
  onSort,
  className,
}: {
  label: string;
  col: SortKey;
  current: SortKey;
  dir: SortDir;
  onSort: (col: SortKey) => void;
  className?: string;
}) {
  const active = current === col;
  return (
    <th
      className={`sortable-th${active ? " sort-active" : ""}${className ? ` ${className}` : ""}`}
      onClick={() => onSort(col)}
    >
      {label}
      <span className="sort-arrow">{active ? (dir === "asc" ? " ▲" : " ▼") : " ⇅"}</span>
    </th>
  );
}

function baseName(path: string | null): string {
  return path ? (path.split(/[/\\]/).pop() ?? path) : "";
}

function hashSortValue(hash: string | undefined): string {
  return hash && hash !== NULL_MD5 ? hash : "￿";
}

/**
 * Workers as a sortable table: one row per slot with the same hash colouring
 * as WorkerCard. Used by the "plain app" themes; the others keep the cards.
 */
export function WorkerTable({ workers }: WorkerTableProps) {
  const [sortKey, setSortKey] = useState<SortKey>("id");
  const [sortDir, setSortDir] = useState<SortDir>("asc");

  const handleSort = (col: SortKey) => {
    if (col === sortKey) {
      setSortDir((d) => (d === "asc" ? "desc" : "asc"));
    } else {
      setSortKey(col);
      setSortDir(col === "progress" || col === "saved" ? "desc" : "asc");
    }
  };

  const sorted = useMemo(() => {
    const copy = [...workers];
    const mul = sortDir === "asc" ? 1 : -1;
    copy.sort((a, b) => {
      switch (sortKey) {
        case "id":
          return mul * (a.id - b.id);
        case "stage":
          return mul * getStageLabel(a.state).localeCompare(getStageLabel(b.state));
        case "file":
          // Idle slots (no file) always sort after named files.
          return mul * (baseName(a.file).toLowerCase() || "￿").localeCompare(baseName(b.file).toLowerCase() || "￿");
        case "progress":
          return mul * ((a.state === "idle" ? -1 : a.percent) - (b.state === "idle" ? -1 : b.percent));
        case "emb":
          return mul * hashSortValue(a.lastEmbeddedMd5).localeCompare(hashSortValue(b.lastEmbeddedMd5));
        case "pre":
          return mul * hashSortValue(a.lastSourceHash).localeCompare(hashSortValue(b.lastSourceHash));
        case "out":
          return mul * hashSortValue(a.lastOutputHash).localeCompare(hashSortValue(b.lastOutputHash));
        case "saved":
          return mul * ((a.lastCompressionPct ?? -1) - (b.lastCompressionPct ?? -1));
        default:
          return 0;
      }
    });
    return copy;
  }, [workers, sortKey, sortDir]);

  return (
    <table className="workers-table interactive">
      <thead>
        <tr>
          <SortHeader label="#"        col="id"       current={sortKey} dir={sortDir} onSort={handleSort} className="col-id" />
          <SortHeader label="Stage"    col="stage"    current={sortKey} dir={sortDir} onSort={handleSort} className="col-stage" />
          <SortHeader label="File"     col="file"     current={sortKey} dir={sortDir} onSort={handleSort} className="col-file" />
          <SortHeader label="Progress" col="progress" current={sortKey} dir={sortDir} onSort={handleSort} className="col-progress" />
          <SortHeader label="EMB"      col="emb"      current={sortKey} dir={sortDir} onSort={handleSort} className="col-hash" />
          <SortHeader label="PRE"      col="pre"      current={sortKey} dir={sortDir} onSort={handleSort} className="col-hash" />
          <SortHeader label="OUT"      col="out"      current={sortKey} dir={sortDir} onSort={handleSort} className="col-hash" />
          <SortHeader label="Saved"    col="saved"    current={sortKey} dir={sortDir} onSort={handleSort} className="col-saved" />
        </tr>
      </thead>
      <tbody>
        {sorted.map((worker) => {
          const isActive = worker.state !== "idle";
          const isConverting = worker.state === "converting";
          const { srcColor, outColor, embColor } = hashColors(
            worker.lastSourceHash,
            worker.lastOutputHash,
            worker.lastEmbeddedMd5
          );
          const showRatio = isConverting && !!worker.ratio;
          const showSaved = !isActive && worker.lastCompressionPct !== undefined;
          return (
            <tr key={worker.id} className={`worker-row ${isActive ? "active" : "idle-row"}`}>
              <td className="col-id worker-id">#{worker.id + 1}</td>
              <td className="col-stage">
                <span className={`worker-stage ${getStageColor(worker.state)}`}>
                  {getStageLabel(worker.state)}
                </span>
              </td>
              <td className="col-file file-name" title={worker.file ?? "idle"}>
                {worker.file ? baseName(worker.file) : "idle"}
              </td>
              <td className="col-progress">
                {isActive && (
                  <span className="worker-progress-row">
                    <span className="progress-indicator worker-progress">
                      <span className="progress-indicator-bar" style={{ width: `${worker.percent}%` }} />
                    </span>
                    <span className="worker-percent">
                      {isConverting && worker.percent > 0 ? `${worker.percent}%` : ""}
                    </span>
                  </span>
                )}
              </td>
              <td className="col-hash">
                <span className={`hash-val ${embColor}`} title={worker.lastEmbeddedMd5 ?? "No embedded MD5"}>
                  {worker.lastEmbeddedMd5 === undefined ? "—" : displayHash(worker.lastEmbeddedMd5)}
                </span>
              </td>
              <td className="col-hash">
                <span className={`hash-val ${srcColor}`} title={worker.lastSourceHash ?? "Source hash unavailable"}>
                  {worker.lastSourceHash ? displayHash(worker.lastSourceHash) : "—"}
                </span>
              </td>
              <td className="col-hash">
                <span className={`hash-val ${outColor}`} title={worker.lastOutputHash ?? "Output hash unavailable"}>
                  {worker.lastOutputHash ? displayHash(worker.lastOutputHash) : "—"}
                </span>
              </td>
              <td className="col-saved">
                {showRatio && (
                  <span className="hash-extra hash-match" title={`Compression ratio ${worker.ratio}`}>≈{worker.ratio}</span>
                )}
                {showSaved && (
                  <span
                    className={`hash-extra hash-saved ${worker.lastCompressionPct! >= 5 ? "hash-saved-good" : "hash-saved-low"}`}
                  >
                    {formatSavedPct(worker.lastCompressionPct!)}
                  </span>
                )}
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}
